//! Two entries share a router; `app` routes to twenty lazily loaded pages and `public` to the
//! first six. Each page imports components and lazily loads two widgets before it renders, and
//! components import shared utilities. Each page load is its entry, its page and its widgets, the
//! load roots listed in `PAGE_LOADS`.
//!
//! The snapshot counts the files each page load downloads under default chunking, under an
//! `entriesAware` group, with `entriesAwareMaxAddedBytesPerSavedRequest`, and with
//! `entriesAwareLoadRoots` added.

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

fn shared_group(
  max_added_bytes_per_saved_request: Option<f64>,
  load_roots: Option<Vec<Vec<String>>>,
) -> CodeSplittingMode {
  CodeSplittingMode::Advanced(ManualCodeSplittingOptions {
    groups: Some(vec![MatchGroup {
      name: MatchGroupName::Static("shared".to_string()),
      test: Some(MatchGroupTest::Regex(HybridRegex::new("(components|utils)/").unwrap())),
      entries_aware: Some(true),
      entries_aware_max_added_bytes_per_saved_request: max_added_bytes_per_saved_request,
      entries_aware_load_roots: load_roots,
      ..Default::default()
    }]),
    ..Default::default()
  })
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
  let configs = [
    ("default chunking", None),
    ("entriesAware", Some(shared_group(None, None))),
    (
      "+ maxAddedBytesPerSavedRequest",
      Some(shared_group(Some(MAX_ADDED_BYTES_PER_SAVED_REQUEST), None)),
    ),
    ("+ loadRoots", Some(shared_group(Some(MAX_ADDED_BYTES_PER_SAVED_REQUEST), Some(load_roots)))),
  ];

  let mut counts_by_config: Vec<Vec<String>> = Vec::new();
  let mut chunk_totals: Vec<usize> = Vec::new();
  for (label, code_splitting) in &configs {
    let chunks = bundle(code_splitting.clone()).await;
    assert_no_static_import_cycle(&chunks);
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
