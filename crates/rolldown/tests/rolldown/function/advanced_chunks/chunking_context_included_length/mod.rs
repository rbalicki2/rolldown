use std::sync::{Arc, Mutex};

use rolldown::{BundlerOptions, InputItem};
use rolldown_common::{CodeSplittingMode, ManualCodeSplittingOptions, MatchGroup, MatchGroupName};
use rolldown_testing::{manual_integration_test, test_config::TestMeta};
use rustc_hash::FxHashMap;

/// `ChunkingContext::get_module_included_length` counts only the statements tree shaking kept.
#[tokio::test(flavor = "multi_thread")]
async fn chunking_context_included_length() {
  let included_lengths: Arc<Mutex<FxHashMap<String, Option<u32>>>> = Arc::default();
  let recorded = Arc::clone(&included_lengths);
  manual_integration_test!()
    .build(TestMeta { expect_executed: true, ..Default::default() })
    .run(BundlerOptions {
      input: Some(vec![InputItem { name: Some("entry".into()), import: "./entry.js".into() }]),
      code_splitting: Some(CodeSplittingMode::Advanced(ManualCodeSplittingOptions {
        groups: Some(vec![MatchGroup {
          name: MatchGroupName::Dynamic(Arc::new(move |module_ids, ctx| {
            let mut recorded = recorded.lock().unwrap();
            // `removed.js` is not a candidate, so its id is derived from the entry's.
            let entry_id = module_ids.iter().find(|id| id.ends_with("entry.js")).unwrap();
            for file in ["entry.js", "lib.js", "removed.js"] {
              let module_id = entry_id.replace("entry.js", file);
              recorded.insert(file.to_string(), ctx.get_module_included_length(&module_id));
            }
            let names = module_ids
              .iter()
              .map(|module_id| module_id.ends_with("lib.js").then(|| "lib".to_string()))
              .collect();
            Box::pin(async move { Ok(names) })
          })),
          ..Default::default()
        }]),
        ..Default::default()
      })),
      ..Default::default()
    })
    .await;

  let included_lengths = included_lengths.lock().unwrap();
  // `export const used = 'used';` is kept; `unused` is not.
  assert_eq!(
    included_lengths["lib.js"],
    Some(u32::try_from("export const used = 'used';".len()).unwrap())
  );
  // The import of `removed.js` is dropped with the module it imports.
  assert_eq!(
    included_lengths["entry.js"],
    Some(
      u32::try_from("import { used } from './lib.js';".len() + "console.log(used);".len()).unwrap()
    )
  );
  // Tree shaking removed the whole module.
  assert_eq!(included_lengths["removed.js"], Some(0));
}
