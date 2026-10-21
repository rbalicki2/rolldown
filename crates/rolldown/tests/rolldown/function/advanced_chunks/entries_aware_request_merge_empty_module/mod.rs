//! `barrel.js` only re-exports `shared.js`, so it renders nothing and no chunk imports the chunk
//! that holds it. A group with the request merge leaves it out, so it never joins the startup
//! graph of the entries that load the merged chunk.

use rolldown::{Bundler, BundlerOptions, InputItem};
use rolldown_common::{
  CodeSplittingMode, ManualCodeSplittingOptions, MatchGroup, MatchGroupName, Output,
};

const FIXTURE_DIR: &str = concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/tests/rolldown/function/advanced_chunks/entries_aware_request_merge_empty_module"
);

#[tokio::test(flavor = "multi_thread")]
async fn entries_aware_request_merge_empty_module() {
  let mut bundler = Bundler::new(BundlerOptions {
    input: Some(vec![
      InputItem { name: Some("main".to_string()), import: "./main.js".to_string() },
      InputItem { name: Some("admin".to_string()), import: "./admin.js".to_string() },
    ]),
    cwd: Some(FIXTURE_DIR.into()),
    code_splitting: Some(CodeSplittingMode::Advanced(ManualCodeSplittingOptions {
      groups: Some(vec![MatchGroup {
        name: MatchGroupName::Static("all".to_string()),
        entries_aware: Some(true),
        entries_aware_max_added_bytes_per_saved_request: Some(1_000_000.0),
        ..Default::default()
      }]),
      ..Default::default()
    })),
    ..Default::default()
  })
  .expect("failed to create bundler");
  let output = bundler.generate().await.expect("build should succeed");
  let group_chunks: Vec<(String, Vec<String>)> = output
    .assets
    .iter()
    .filter_map(|output| match output {
      Output::Chunk(chunk) if chunk.filename.starts_with("all") => Some((
        chunk.filename.to_string(),
        chunk.module_ids.iter().map(ToString::to_string).collect(),
      )),
      _ => None,
    })
    .collect();
  assert!(
    group_chunks
      .iter()
      .any(|(_, module_ids)| module_ids.iter().any(|id| id.ends_with("/shared.js"))),
    "`shared.js` should be in a group chunk: {group_chunks:?}"
  );
  for (file_name, module_ids) in &group_chunks {
    assert!(
      !module_ids.iter().any(|id| id.ends_with("/barrel.js")),
      "{file_name} holds `barrel.js`, which renders nothing"
    );
  }
}
