use std::{borrow::Cow, ops::Deref, pin::Pin, sync::Arc};

use arcstr::ArcStr;
use rolldown_error::BuildResult;
use rolldown_utils::js_regex::HybridRegex;
use rustc_hash::FxHashMap;
#[cfg(feature = "deserialize_bundler_options")]
use schemars::JsonSchema;
#[cfg(feature = "deserialize_bundler_options")]
use serde::{Deserialize, Deserializer};

use crate::{InvalidateJsSideCache, ModuleInfo, ModuleTag, SharedModuleInfoDashMap};

use super::inline_common_chunks_options::InlineCommonChunksOptions;

/// Schema-only enum for built-in module tags. Used by `schemars` to generate
/// a restricted JSON Schema for the `tags` field. Not used at runtime.
#[cfg(feature = "deserialize_bundler_options")]
#[derive(JsonSchema)]
#[expect(dead_code)]
enum BuiltinModuleTag {
  #[schemars(rename = "$initial")]
  Initial,
  // Phase 2:
  // #[schemars(rename = "$lazy")]
  // Lazy,
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(
  feature = "deserialize_bundler_options",
  derive(Deserialize, JsonSchema),
  serde(rename_all = "camelCase", deny_unknown_fields)
)]
pub struct ManualCodeSplittingOptions {
  pub min_share_count: Option<u32>,
  pub min_size: Option<f64>,
  pub max_size: Option<f64>,
  pub min_module_size: Option<f64>,
  pub max_module_size: Option<f64>,
  pub include_dependencies_recursively: Option<bool>,
  pub groups: Option<Vec<MatchGroup>>,
  /// See internal-docs/inline-common-chunks/design.md. Experimental.
  pub experimental_inline_common_chunks: Option<InlineCommonChunksOptions>,
  #[cfg_attr(feature = "deserialize_bundler_options", serde(skip), schemars(skip))]
  pub internal_invalidate_module_info_cache: Option<InvalidateJsSideCache>,
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(
  feature = "deserialize_bundler_options",
  derive(Deserialize, JsonSchema),
  serde(rename_all = "camelCase", deny_unknown_fields)
)]
pub struct MatchGroup {
  #[cfg_attr(
    feature = "deserialize_bundler_options",
    serde(deserialize_with = "deserialize_match_group_name"),
    schemars(with = "String")
  )]
  pub name: MatchGroupName,
  #[cfg_attr(
    feature = "deserialize_bundler_options",
    serde(deserialize_with = "deserialize_test", default),
    schemars(with = "Option<String>")
  )]
  pub test: Option<MatchGroupTest>,
  // pub share_count: Option<u32>,
  pub priority: Option<u32>,
  pub min_size: Option<f64>,
  pub max_size: Option<f64>,
  pub min_share_count: Option<u32>,
  pub min_module_size: Option<f64>,
  pub max_module_size: Option<f64>,
  pub entries_aware: Option<bool>,
  /// Only effective when `entriesAware` is set to `true`.
  pub entries_aware_merge_threshold: Option<f64>,
  /// Only effective when `entriesAware` is set to `true`. Merges subgroups while the included
  /// bytes a merge adds to loads, per request it saves them, stay within this budget.
  /// See internal-docs/manual-code-splitting/implementation.md#entriesawaremaxaddedbytespersavedrequest.
  pub entries_aware_max_added_bytes_per_saved_request: Option<f64>,
  /// Only effective with `entries_aware_max_added_bytes_per_saved_request`. Each load root lists
  /// module ids of entries or dynamically imported modules that load together, such as an entry and
  /// the route chunks a page imports before it is usable. Each root is one more load the merge
  /// counts.
  /// See internal-docs/manual-code-splitting/implementation.md#entriesawareloadroots.
  pub entries_aware_load_roots: Option<Vec<Vec<String>>>,
  /// Only effective with `entries_aware_max_added_bytes_per_saved_request`. When `true`, the
  /// requests a merge saves or adds count only on load roots.
  /// See internal-docs/manual-code-splitting/implementation.md#cost-weighting.
  pub entries_aware_count_load_roots_only: Option<bool>,
  /// Only effective with `entries_aware_max_added_bytes_per_saved_request`. When `true`, the bytes
  /// and requests a merge adds to the load of a single dynamic entry cost nothing.
  /// See internal-docs/manual-code-splitting/implementation.md#cost-weighting.
  pub entries_aware_free_dynamic_entry_loads: Option<bool>,
  /// Only effective with `entries_aware_max_added_bytes_per_saved_request`. Module ids of entries
  /// whose startup code must not grow: no merge adds a module of the group to what one of these
  /// entries loads on its own.
  /// See internal-docs/manual-code-splitting/implementation.md#cost-weighting.
  pub entries_aware_no_growth_entries: Option<Vec<String>>,
  /// Only effective with `entries_aware_max_added_bytes_per_saved_request`. Sets of globs of entry
  /// and dynamically imported module ids. A module that only the entries of one set lead to never
  /// shares a chunk with a module that other loads download.
  /// See internal-docs/manual-code-splitting/implementation.md#entriesawareisolate.
  pub entries_aware_isolate: Option<Vec<Vec<String>>>,
  /// Filter modules by tags. Only modules with all specified tags are captured.
  /// See internal-docs/module-tags/implementation.md
  #[cfg_attr(
    feature = "deserialize_bundler_options",
    schemars(with = "Option<Vec<BuiltinModuleTag>>")
  )]
  pub tags: Option<Vec<ModuleTag>>,
  pub include_dependencies_recursively: Option<bool>,
}

/// Classifies a whole batch of module ids in one call.
///
/// The returned `Vec` must match `module_ids` by index. The caller rejects any other length.
type MatchGroupTestFn = dyn Fn(
    /* module ids */ Vec<String>,
  ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<bool>>> + Send + 'static>>
  + Send
  + Sync;

#[derive(derive_more::Debug, Clone)]
pub enum MatchGroupTest {
  Regex(HybridRegex),
  #[debug("Function")]
  Function(Arc<MatchGroupTestFn>),
}

#[cfg(feature = "deserialize_bundler_options")]
fn deserialize_test<'de, D>(deserializer: D) -> Result<Option<MatchGroupTest>, D::Error>
where
  D: Deserializer<'de>,
{
  let deserialized = Option::<String>::deserialize(deserializer)?;
  let transformed = deserialized
    .map(|inner| HybridRegex::new(&inner))
    .transpose()
    .map_err(|e| serde::de::Error::custom(format!("failed to deserialize {e} to HybridRegex")))?;
  Ok(transformed.map(MatchGroupTest::Regex))
}

/// Names a whole batch of module ids in one call. The returned `Vec` follows the same rule as
/// [`MatchGroupTestFn`].
type MatchGroupNameFn = dyn Fn(
    /* module ids */ Vec<String>,
    /* chunking context */ &ChunkingContext,
  ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Option<String>>>> + Send + 'static>>
  + Send
  + Sync;

#[derive(derive_more::Debug, Clone)]
pub enum MatchGroupName {
  Static(String),
  #[debug("Function")]
  Dynamic(Arc<MatchGroupNameFn>),
}

impl MatchGroupName {
  /// Resolves the group name of every id in `module_ids`, in the same order. The result always
  /// has the same length as `module_ids`.
  pub async fn values<'a>(
    &'a self,
    ctx: &ChunkingContext,
    module_ids: Vec<String>,
  ) -> BuildResult<Vec<Option<Cow<'a, str>>>> {
    match self {
      Self::Static(name) => Ok(vec![Some(Cow::Borrowed(name.as_str())); module_ids.len()]),
      Self::Dynamic(func) => {
        let expected = module_ids.len();
        let names = func(module_ids, ctx).await?;
        if names.len() != expected {
          return Err(
            anyhow::anyhow!(
              "a `codeSplitting` group `name` function returned {} names for {expected} modules",
              names.len()
            )
            .into(),
          );
        }
        Ok(names.into_iter().map(|name| name.map(Cow::Owned)).collect())
      }
    }
  }
}

impl Default for MatchGroupName {
  fn default() -> Self {
    Self::Static(String::new())
  }
}

#[cfg(feature = "deserialize_bundler_options")]
fn deserialize_match_group_name<'de, D>(deserializer: D) -> Result<MatchGroupName, D::Error>
where
  D: Deserializer<'de>,
{
  let deserialized = String::deserialize(deserializer)?;
  Ok(MatchGroupName::Static(deserialized))
}

#[derive(Debug, Clone)]
pub struct ChunkingContext(Arc<ChunkingContextImpl>);

impl ChunkingContext {
  pub fn new(
    module_infos: SharedModuleInfoDashMap,
    included_lengths: FxHashMap<ArcStr, u32>,
  ) -> Self {
    Self(Arc::new(ChunkingContextImpl { module_infos, included_lengths }))
  }
}

impl Deref for ChunkingContext {
  type Target = ChunkingContextImpl;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

#[derive(Debug)]
pub struct ChunkingContextImpl {
  module_infos: SharedModuleInfoDashMap,
  /// Included length of every normal module, keyed by module id.
  /// See internal-docs/manual-code-splitting/implementation.md#included-length.
  included_lengths: FxHashMap<ArcStr, u32>,
}

impl ChunkingContextImpl {
  pub fn get_module_info(&self, module_id: &str) -> Option<Arc<ModuleInfo>> {
    self.module_infos.get(module_id).map(|v| Arc::<ModuleInfo>::clone(v.value()))
  }

  /// Returns the included length of a normal module, or `None` for an external or unknown id.
  pub fn get_module_included_length(&self, module_id: &str) -> Option<u32> {
    self.included_lengths.get(module_id).copied()
  }
}
