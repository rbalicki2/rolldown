# Manual Code Splitting

## Summary

Manual code splitting lets users define chunk boundaries via `manualCodeSplitting.groups`. Each group has a `name`, a `test` pattern to match modules, and optional size/priority controls. Matched modules (and optionally their dependencies) are pulled into dedicated chunks instead of being split by the automatic algorithm.

## Important features

### JavaScript module information cache

All dynamic group-name callbacks in one manual splitting pass share a `ChunkingContextImpl`. It caches complete JavaScript `ModuleInfo` objects by module ID after their first native lookup. Scanning has completed before this pass, so dependency arrays and entry flags are stable. `meta` retains its shared JavaScript reference, `moduleSideEffects` reads and writes the existing module-option store, and `code` remains a lazy getter.

The binding supplies a private `internalInvalidateModuleInfoCache` callback on the manual splitting options. `apply_manual_code_splitting` invokes it after `split` returns, including errors. If both splitting and invalidation fail, the splitting error takes precedence. The callback drops the cache and disables caching on retained contexts; `moduleSideEffects` continues to read and write the shared module-option store. The next splitting pass creates a fresh context. The plugin-context API and graph construction do not use this cache.

### Included length

`ChunkingContext.getModuleInfo(id).includedLength` (Rust: `ChunkingContextImpl::get_module_included_length`) is the module's source length after tree shaking. `compute_included_lengths` in `manual_code_splitting.rs` sums the spans of the top-level statements whose `stmt_info_included` bit is set. Statement info `i + 1` describes `program.body[i]` (statement info `0` is the namespace object), the same pairing `remove_unused_top_level_stmt` in the module finalizer uses. Modules tree shaking removed and external modules get `0`.

Manual splitting runs in the generate stage after the link stage's `include_statements`, so inclusion is known, but rendering has not happened, so the value estimates `RenderedModule.renderedLength`:

- It counts import and export declarations, which rendering rewrites into cross-chunk imports and exports.
- It does not count what rendering adds: CommonJS and ESM wrappers, namespace objects, runtime helpers, and identifier renames.
- Statements generated in the link stage (lazy-export JSON declarations, wrapper statements) have no source span and count as `0`.
- Statements included after chunking are not counted: simulated facade inclusion in `optimize_facade_entry_chunks` replays inclusion, and the finalizer keeps statements that `OrderWrapState::forces_runtime_stmt` forces under `strictExecutionOrder`.

The lengths are computed once per splitting pass, and only when a group has a function `name`.

### `entriesAware`

When `entriesAware: true`, a group's modules are further split by **which entry points can reach them**. This produces per-entry-set chunks instead of one monolithic group chunk.

#### How it works

Each module has a **bitset** representing which entries can reach it. After collecting all modules into the group, we split them into subgroups by their bitset pattern:

```
Given: 3 entries (A, B, C) and a group matching shared-*.js

Module reachability:
  shared-abc.js  →  bits = {A, B, C}   (all entries)
  shared-ab.js   →  bits = {A, B}      (entries A and B)
  shared-a.js    →  bits = {A}         (entry A only)

Step 1: Build one flat group with all matching modules

  ┌─────────────────────────────────────────┐
  │ vendor group (flat)                     │
  │  shared-abc.js  shared-ab.js  shared-a  │
  └─────────────────────────────────────────┘

Step 2: Split by bitset → subgroups

  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
  │ bits = {A, B, C} │  │ bits = {A, B}    │  │ bits = {A}       │
  │ shared-abc.js    │  │ shared-ab.js     │  │ shared-a.js      │
  └──────────────────┘  └──────────────────┘  └──────────────────┘

Step 3: Each subgroup → chunk

  vendor~entry-a~entry-b~entry-c.js   (loaded by all)
  vendor~entry-a~entry-b.js           (loaded by A and B only)
  vendor~entry-a.js                   (loaded by A only)
```

Entry A loads all three vendor chunks. Entry B loads the first two. Entry C loads only the first. Each entry only downloads what it actually needs.

#### Why flat-then-split matters

The split must happen **after** collecting all modules into a flat group. If subgroups are created during the build phase (per module's own bits), `includeDependenciesRecursively` adds shared dependencies to each subgroup independently:

```
BAD: subgroups during build (dependencies duplicated)

  lib-a.js (bits={A}) matches → subgroup {A}
    └─ deps: shared-dep.js added to subgroup {A}

  lib-b.js (bits={B}) matches → subgroup {B}
    └─ deps: shared-dep.js added to subgroup {B}  ← DUPLICATE

  subgroup {A} = [lib-a, shared-dep]  size: 150
  subgroup {B} = [lib-b, shared-dep]  size: 150
                          ^^^^^^^^^^
                    counted twice → inflated sizes

GOOD: flat group first, split after

  flat group = [lib-a, lib-b, shared-dep]   (each module once)

  split by bits:
    {A}     → [lib-a]       size: 30
    {B}     → [lib-b]       size: 30
    {A, B}  → [shared-dep]  size: 100   ← counted once, correct size
```

Inflated sizes break `entriesAwareMergeThreshold` — subgroups that should merge (because they're actually small) appear too large.

#### `entriesAwareMergeThreshold`

Splitting by bitset can produce many tiny subgroups. `entriesAwareMergeThreshold` merges subgroups below a size threshold into their nearest neighbor (by bitset similarity):

```
Before merge (threshold = 50):
  {A}     → [lib-a]       size: 30  ← below threshold
  {B}     → [lib-b]       size: 30  ← below threshold
  {A, B}  → [shared-dep]  size: 100

Merge: lib-a (30 < 50) merges into {A, B} (smallest symmetric difference)
Merge: lib-b (30 < 50) merges into {A, B}

After merge:
  {A, B}  → [lib-a, lib-b, shared-dep]  size: 160
```

The merge algorithm uses a min-heap to process smallest subgroups first. For each candidate, it finds the best target by minimizing **symmetric difference** of bitsets (prefer similar entry-point patterns), with size as tiebreaker. Both candidate and target ties are finally resolved by subgroup key. When merging is enabled, the group's modules are sorted by execution order before assigning subgroup keys, so module loading order cannot affect merge decisions. Sorting is limited to the group's modules and skipped when the merge threshold is disabled.

#### `entriesAwareMaxAddedBytesPerSavedRequest`

`entriesAwareMergeThreshold` merges by size alone, so it cannot tell a merge that saves many loads a request from one that saves none. This option merges by requests saved, in `entries_aware_request_merge.rs`. It runs after the bitset split and before `entriesAwareMergeThreshold`.

- **Loads.** Every entry, including every dynamic entry, is a load. A load downloads the subgroups whose bits contain its entry, plus everything those subgroups import, transitively.
- **Subgroup imports.** A subgroup imports another when one of its modules reaches one of the other's modules through `load_dependencies` (kept static imports). Paths through modules outside the group are followed and marked, since those modules land in other chunks.
- **Cost.** Merging subgroups `X` and `Y` saves one request on each load that downloads both. A load that downloads only one of them downloads the merged chunk and what it imports, so it gains bytes and possibly requests. The cost is the included bytes added over all loads (see [Included length](#included-length)) divided by the requests saved minus the requests added. A merge with no net saving is not allowed.
- **Loop.** Each step makes the allowed merge with the lowest cost, ties broken by subgroup key, while that cost is at most the option's value.
- **Incremental pricing.** `plan_merges` keeps, per subgroup, its closure (itself and what it imports, transitively) and the loads that download it, and per load the subgroups it downloads, all as dense bitsets. A pair's cost reads only those: the loads that download both, and for each load that downloads one, the other's closure minus what the load downloads. A priority queue holds every pair some load downloads together, keyed by a lower bound (the other subgroup's bytes on each one-sided load, over the gross saved requests) until the pair reaches the front, where its exact cost replaces the bound. After a merge, only pairs with a subgroup whose closure changed (ancestors of the two merged subgroups), that a load started downloading, or that imports one of those are priced again; a version per subgroup marks their old queue entries stale. The order of merges is the same as pricing every pair against every load after each merge, which a unit test checks on random graphs.
- **Entry sets.** Each subgroup has the set of user-defined entries that load it (`compute_module_loaders`: user-defined entries from which one of its modules is reachable through `load_dependencies` and through dynamic imports whose statement tree shaking kept, read from the dynamic entry's `related_stmt_infos`). Only subgroups with equal sets merge, so a merge never puts code only one user-defined entry loads into a chunk another user-defined entry loads.
- **Entry modules.** The group drops user-defined and emitted entry modules before splitting, so they stay in their entry chunks. A captured entry module would otherwise move into a shared chunk and leave its entry chunk a facade. It also drops modules whose bits name one entry alone (`keep_entry_chunk_modules`) when the entry is user-defined, where default chunking keeps them in the entry's chunk and a chunk of their own would only add a request to the entry's load, or when the entry's chunk keeps a module the group does not capture, such as a stylesheet the group's `test` leaves out: a group chunk importing that module would import the entry's chunk, which imports the group chunk back for the entry's exports. Finally it drops modules that render nothing (`renders_nothing`: no side effects, namespace object not included, every kept top-level statement an import or a re-export). No chunk imports the chunk that holds such a module, but the merge follows `load_dependencies`, which still lists it, so in a merged chunk it would join the startup graph of every load of that chunk without adding a byte.
- **Cycles.** A merge is refused when one subgroup reaches the other through a third subgroup or through a module outside the group: the merged chunk and the chunk in between would import each other.
- **Top-level await.** A subgroup with a module whose `is_tla_or_contains_tla_dependency` is set never merges. A lazy chunk importing the merged chunk could wait for top-level await that waits for the lazy chunk; common chunk merging is skipped for such builds for the same reason.

Known gaps:

- Bytes and requests count only this group's subgroups. Modules outside the group that a merge makes a load download are not counted. With `includeDependenciesRecursively` (the default) the group holds its modules' dependencies, so such modules come only from groups emitted earlier.
- The cycle check sees modules outside the group one by one. Two outside modules that end up in one chunk can still close a cycle the check does not see.
- A load that downloads a merged chunk also runs the top-level code of the modules it did not need, as with any manual group.
- A merge reprices every pair that has a subgroup whose closure changed or that a load started downloading. On graphs shaped like an app's (`thousands_of_dynamic_entries`: 3,000 subgroups, 2,010 loads) the search takes well under a second in a release build. When most loads download most subgroups, most pairs are repriced after every merge: on a random import graph of 3,000 subgroups and 2,010 loads the search took about ten seconds, and on one whose imports chain through nearby subgroups, so that each load downloads a large part of the graph, it did not finish in 30 minutes.

#### `entriesAwareLoadRoots`

A page load is rarely one entry. A page downloads its entry's static graph, then its route chunks and the lazy components it renders first, each a separate dynamic entry. With only per-entry loads, two subgroups that no single entry downloads together never merge, even when every load of the page downloads both. Each load root lists entries that load together, and `entries_aware_loads` adds it to the per-entry loads, so the merge counts the requests it saves that page load. Roots are counted like any other load: they save requests and they pay for added bytes. Ids match an entry's module id or stable id, a leading `./` ignored; an id that matches no entry is an `INVALID_OPTION` error.

`function/advanced_chunks/entries_aware_request_explosion` measures both options on a two-entry app with twenty lazily loaded pages that each lazily load two widgets.

## Related

- [code-splitting.md](../code-splitting/implementation.md) — automatic code splitting
