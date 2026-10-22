//! Two entries share a router; `app` routes to twenty lazily loaded pages and `public` to the
//! first six. Each page imports components and lazily loads two widgets before it renders, and
//! components import shared utilities. Each page load is its entry, its page and its widgets, the
//! load roots listed in `PAGE_LOADS`.
//!
//! The snapshot counts the files each page load downloads under default chunking, under an
//! `entriesAware` group, with `entriesAwareMaxAddedBytesPerSavedRequest`, with
//! `entriesAwareLoadRoots` added, and with requests counted on load roots only, dynamic entries'
//! loads free and both entries' startup code fixed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use rolldown::{Bundler, BundlerOptions, InputItem};
use rolldown_common::{
  CodeSplittingMode, ManualCodeSplittingOptions, MatchGroup, MatchGroupName, MatchGroupTest, Output,
};
use rolldown_utils::js_regex::HybridRegex;

const FIXTURE_DIR: &str = concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/tests/rolldown/function/advanced_chunks/entries_aware_request_explosion"
);

/// Each page load: its entry, the page it renders and the widgets the page loads.
const PAGE_LOADS: [&[&str]; 6] = [
  &["app.js", "pages/page-00.js", "widgets/w-01.js", "widgets/w-04.js"],
  &["app.js", "pages/page-01.js", "widgets/w-04.js", "widgets/w-09.js"],
  &["app.js", "pages/page-02.js", "widgets/w-02.js", "widgets/w-07.js"],
  &["app.js", "pages/page-07.js", "widgets/w-03.js", "widgets/w-10.js"],
  &["public.js", "pages/page-00.js", "widgets/w-01.js", "widgets/w-04.js"],
  &["public.js", "pages/page-01.js", "widgets/w-04.js", "widgets/w-09.js"],
];

/// Components only pages 06 to 19 and their widgets import. `public` routes to pages 00 to 05
/// only, so its page loads must never download these.
const APP_ONLY_COMPONENTS: [&str; 10] =
  ["c-03", "c-10", "c-12", "c-21", "c-23", "c-27", "c-29", "c-34", "c-35", "c-38"];

const MAX_ADDED_BYTES_PER_SAVED_REQUEST: f64 = 1000.0;

struct OutputChunkSummary {
  facade_module_id: Option<String>,
  module_ids: Vec<String>,
  imports: Vec<String>,
  code_length: usize,
}

async fn bundle(code_splitting: Option<CodeSplittingMode>) -> BTreeMap<String, OutputChunkSummary> {
  let mut bundler = Bundler::new(BundlerOptions {
    input: Some(vec![
      InputItem { name: Some("app".to_string()), import: "./app.js".to_string() },
      InputItem { name: Some("public".to_string()), import: "./public.js".to_string() },
    ]),
    cwd: Some(FIXTURE_DIR.into()),
    code_splitting,
    ..Default::default()
  })
  .expect("failed to create bundler");
  bundler
    .generate()
    .await
    .expect("build should succeed")
    .assets
    .into_iter()
    .filter_map(|output| match output {
      Output::Chunk(chunk) => Some((
        chunk.filename.to_string(),
        OutputChunkSummary {
          facade_module_id: chunk.facade_module_id.as_ref().map(ToString::to_string),
          module_ids: chunk.module_ids.iter().map(ToString::to_string).collect(),
          imports: chunk.imports.iter().map(ToString::to_string).collect(),
          code_length: chunk.code.len(),
        },
      )),
      Output::Asset(_) => None,
    })
    .collect()
}

fn shared_group(configure: impl FnOnce(&mut MatchGroup)) -> CodeSplittingMode {
  let mut group = MatchGroup {
    name: MatchGroupName::Static("shared".to_string()),
    test: Some(MatchGroupTest::Regex(HybridRegex::new("(components|utils)/").unwrap())),
    entries_aware: Some(true),
    ..Default::default()
  };
  configure(&mut group);
  CodeSplittingMode::Advanced(ManualCodeSplittingOptions {
    groups: Some(vec![group]),
    ..Default::default()
  })
}

/// Returns the ids of the modules in `files`.
fn module_ids(
  chunks: &BTreeMap<String, OutputChunkSummary>,
  files: &BTreeSet<String>,
) -> BTreeSet<String> {
  files.iter().flat_map(|file_name| chunks[file_name].module_ids.iter().cloned()).collect()
}

/// Returns the files a page load downloads before its page can render: the chunks of its entry,
/// page and widgets and everything they import statically, transitively.
fn files_loaded(chunks: &BTreeMap<String, OutputChunkSummary>, load: &[&str]) -> BTreeSet<String> {
  let mut loaded = BTreeSet::new();
  let mut pending: Vec<String> = load
    .iter()
    .map(|module| {
      chunks
        .iter()
        .find(|(_, chunk)| {
          chunk.facade_module_id.as_ref().is_some_and(|id| id.ends_with(&format!("/{module}")))
        })
        .map(|(file_name, _)| file_name.clone())
        .unwrap_or_else(|| panic!("no chunk for {module}"))
    })
    .collect();
  while let Some(file_name) = pending.pop() {
    if loaded.insert(file_name.clone()) {
      pending.extend(chunks[&file_name].imports.iter().cloned());
    }
  }
  loaded
}

/// Panics when two chunks import each other statically, directly or through other chunks.
fn assert_no_static_import_cycle(chunks: &BTreeMap<String, OutputChunkSummary>) {
  fn visit<'a>(
    file_name: &'a str,
    chunks: &'a BTreeMap<String, OutputChunkSummary>,
    stack: &mut Vec<&'a str>,
    done: &mut BTreeSet<&'a str>,
  ) {
    if done.contains(file_name) {
      return;
    }
    assert!(!stack.contains(&file_name), "static import cycle: {stack:?} -> {file_name}");
    stack.push(file_name);
    for imported in &chunks[file_name].imports {
      visit(imported, chunks, stack, done);
    }
    stack.pop();
    done.insert(file_name);
  }
  let mut done = BTreeSet::new();
  for file_name in chunks.keys() {
    visit(file_name, chunks, &mut Vec::new(), &mut done);
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn entries_aware_request_explosion() {
  let load_roots: Vec<Vec<String>> =
    PAGE_LOADS.iter().map(|load| load.iter().map(ToString::to_string).collect()).collect();
  let with_merge = |group: &mut MatchGroup| {
    group.entries_aware_max_added_bytes_per_saved_request = Some(MAX_ADDED_BYTES_PER_SAVED_REQUEST);
  };
  let with_load_roots = |group: &mut MatchGroup| {
    with_merge(group);
    group.entries_aware_load_roots = Some(load_roots.clone());
  };
  let with_page_load_weighting = |group: &mut MatchGroup| {
    with_load_roots(group);
    group.entries_aware_count_load_roots_only = Some(true);
    group.entries_aware_free_dynamic_entry_loads = Some(true);
    group.entries_aware_no_growth_entries =
      Some(vec!["app.js".to_string(), "public.js".to_string()]);
  };
  let configs = [
    ("default chunking", None),
    ("entriesAware", Some(shared_group(|_| {}))),
    ("+ maxAddedBytesPerSavedRequest", Some(shared_group(with_merge))),
    ("+ loadRoots", Some(shared_group(with_load_roots))),
    ("+ page-load weighting", Some(shared_group(with_page_load_weighting))),
  ];
  // The configurations whose entries' startup code must not grow.
  let no_growth_configs = ["+ page-load weighting"];
  let mut startup_modules_without_merge: Option<Vec<BTreeSet<String>>> = None;
  // The configurations that count requests on load roots only, which leave every module no page
  // load downloads to default chunking.
  let root_scoped_configs = ["+ page-load weighting"];
  let mut default_chunk_by_module: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

  let mut counts_by_config: Vec<Vec<String>> = Vec::new();
  let mut chunk_totals: Vec<usize> = Vec::new();
  for (label, code_splitting) in &configs {
    let chunks = bundle(code_splitting.clone()).await;
    assert_no_static_import_cycle(&chunks);
    let startup_modules: Vec<BTreeSet<String>> = ["app.js", "public.js"]
      .iter()
      .map(|entry| module_ids(&chunks, &files_loaded(&chunks, &[entry])))
      .collect();
    let chunk_by_module: BTreeMap<String, BTreeSet<String>> = chunks
      .values()
      .flat_map(|chunk| {
        let module_ids: BTreeSet<String> = chunk.module_ids.iter().cloned().collect();
        chunk.module_ids.iter().map(move |id| (id.clone(), module_ids.clone()))
      })
      .collect();
    if *label == "default chunking" {
      default_chunk_by_module = chunk_by_module.clone();
    }
    if root_scoped_configs.contains(label) {
      let downloaded: BTreeSet<String> = PAGE_LOADS
        .iter()
        .flat_map(|load| module_ids(&chunks, &files_loaded(&chunks, load)))
        .collect();
      for (module_id, chunk_modules) in &chunk_by_module {
        if !downloaded.contains(module_id) {
          assert_eq!(
            Some(chunk_modules),
            default_chunk_by_module.get(module_id),
            "{label}: {module_id}, which no page load downloads, is not chunked as by default chunking"
          );
        }
      }
    }
    if *label == "entriesAware" {
      startup_modules_without_merge = Some(startup_modules.clone());
    }
    if no_growth_configs.contains(label) {
      assert_eq!(
        Some(&startup_modules),
        startup_modules_without_merge.as_ref(),
        "{label}: a merge added modules to an entry's startup code"
      );
    }
    let counts = PAGE_LOADS
      .iter()
      .map(|load| {
        let loaded = files_loaded(&chunks, load);
        if load[0] == "public.js" {
          for file_name in &loaded {
            for module_id in &chunks[file_name].module_ids {
              assert!(
                !APP_ONLY_COMPONENTS.iter().any(|component| module_id.contains(component)),
                "{label}: {load:?} downloads {module_id}, which only `app` pages import"
              );
            }
          }
        }
        let bytes: usize = loaded.iter().map(|file_name| chunks[file_name].code_length).sum();
        format!("{} files, {bytes} B", loaded.len())
      })
      .collect();
    counts_by_config.push(counts);
    chunk_totals.push(chunks.len());
  }

  let mut table = String::from(
    "Files each page load downloads and their unminified bytes, by configuration.\n\n",
  );
  let mut write_row = |label: &str, cells: &mut dyn Iterator<Item = String>| {
    let _ = write!(table, "{label:<32}");
    for cell in cells {
      let _ = write!(table, "{cell:>32}");
    }
    table.push('\n');
  };
  write_row("page load", &mut configs.iter().map(|(label, _)| (*label).to_string()));
  for (load_index, load) in PAGE_LOADS.iter().enumerate() {
    write_row(
      &format!("{} + {}", load[0], load[1]),
      &mut counts_by_config.iter().map(|counts| counts[load_index].clone()),
    );
  }
  write_row("chunks in the build", &mut chunk_totals.iter().map(ToString::to_string));

  insta::with_settings!({ snapshot_path => FIXTURE_DIR, prepend_module_to_snapshot => false }, {
    insta::assert_snapshot!("chunk_counts", table);
  });
}
