//! Request-aware merging of `entriesAware` subgroups.
//! See internal-docs/manual-code-splitting/implementation.md#entriesawaremaxaddedbytespersavedrequest.

use std::{
  cmp::{Ordering, Reverse},
  collections::{BinaryHeap, VecDeque},
};

use oxc_index::IndexVec;
use rolldown_common::{EntryPoint, Module, ModuleIdx};
use rolldown_utils::BitSet;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::stages::link_stage::LinkStageOutput;

use super::manual_code_splitting::{EntriesAwareSubgroup, merge_subgroups};

/// What the merge reads besides the subgroups.
pub(super) struct RequestMergeContext<'a> {
  pub link_output: &'a LinkStageOutput,
  /// Included length of every module. See `compute_included_lengths`.
  pub included_lengths: &'a IndexVec<ModuleIdx, u32>,
  /// For each module, the user-defined entries that load it. See [`compute_module_loaders`].
  pub module_loaders: &'a IndexVec<ModuleIdx, BitSet>,
  /// Each load is the set of entry indices whose chunks load together.
  pub loads: &'a [BitSet],
}

/// Merges subgroups, cheapest first, while the included bytes a merge adds to the loads that
/// download only one of the two subgroups, divided by the requests it saves, stay within
/// `max_added_bytes_per_saved_request`. A merge saves one request on each load that downloads both
/// subgroups and adds one for each subgroup a load starts downloading because the merged subgroup
/// imports it. Subgroups that different user-defined entries load never merge, and no merge makes
/// two chunks import each other.
pub(super) fn merge_entries_aware_subgroups_by_requests(
  subgroups: &mut FxHashMap<u32, EntriesAwareSubgroup>,
  ctx: &RequestMergeContext<'_>,
  max_added_bytes_per_saved_request: f64,
) {
  let mut keys: Vec<u32> = subgroups.keys().copied().collect();
  keys.sort_unstable();
  let (nodes, loads) = build_merge_input(subgroups, &keys, ctx);
  for (host, absorbed) in plan_merges(&nodes, &loads, max_added_bytes_per_saved_request) {
    merge_subgroups(
      subgroups,
      keys[absorbed as usize],
      keys[host as usize],
      &ctx.link_output.module_table,
    );
  }
}

/// One subgroup as the merge sees it. Subgroups are numbered in key order.
struct NodeInput {
  included_bytes: f64,
  /// Numbers the set of user-defined entries that load the subgroup. Only nodes with the same
  /// number merge.
  loaders: u32,
  /// `false` when a module of the subgroup has or waits for top-level await. A chunk a lazy chunk
  /// imports could then wait for an entry that waits for the lazy chunk, so it never merges, as
  /// common chunk merging is skipped for such builds.
  mergeable: bool,
  /// Subgroups this subgroup's modules import. The value is `true` when some path to the
  /// imported subgroup passes through a module outside the group, which lands in another chunk.
  imports: FxHashMap<u32, bool>,
}

/// Returns the merge's nodes, numbered in `keys` order, and for each load with at least one
/// subgroup, the nodes holding modules its entries reach directly.
fn build_merge_input(
  subgroups: &FxHashMap<u32, EntriesAwareSubgroup>,
  keys: &[u32],
  ctx: &RequestMergeContext<'_>,
) -> (Vec<NodeInput>, Vec<Vec<u32>>) {
  let node_by_module: FxHashMap<ModuleIdx, u32> = keys
    .iter()
    .enumerate()
    .flat_map(|(node, key)| {
      let node = u32::try_from(node).expect("Too many subgroups");
      subgroups[key].modules.iter().map(move |module_idx| (*module_idx, node))
    })
    .collect();
  let mut loader_numbers: FxHashMap<BitSet, u32> = FxHashMap::default();
  let mut nodes_by_entry: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
  let nodes = keys
    .iter()
    .enumerate()
    .map(|(node, key)| {
      let node = u32::try_from(node).expect("Too many subgroups");
      let subgroup = &subgroups[key];
      for entry_index in subgroup.bits.index_of_one() {
        nodes_by_entry.entry(entry_index).or_default().push(node);
      }
      let mut loaders: Option<BitSet> = None;
      for module_idx in &subgroup.modules {
        match &mut loaders {
          Some(loaders) => loaders.union(&ctx.module_loaders[*module_idx]),
          None => loaders = Some(ctx.module_loaders[*module_idx].clone()),
        }
      }
      let next_number = u32::try_from(loader_numbers.len()).expect("Too many subgroups");
      let loaders = *loader_numbers.entry(loaders.unwrap_or_default()).or_insert(next_number);
      NodeInput {
        included_bytes: subgroup
          .modules
          .iter()
          .map(|module_idx| f64::from(ctx.included_lengths[*module_idx]))
          .sum(),
        loaders,
        mergeable: !subgroup
          .modules
          .iter()
          .any(|module_idx| ctx.link_output.metas[*module_idx].is_tla_or_contains_tla_dependency),
        imports: collect_imported_nodes(node, &subgroup.modules, &node_by_module, ctx),
      }
    })
    .collect();
  let loads = ctx
    .loads
    .iter()
    .filter_map(|load| {
      let seeds: FxHashSet<u32> = load
        .index_of_one()
        .filter_map(|entry_index| nodes_by_entry.get(&entry_index))
        .flatten()
        .copied()
        .collect();
      (!seeds.is_empty()).then(|| seeds.into_iter().collect())
    })
    .collect();
  (nodes, loads)
}

/// Follows the kept static imports (`load_dependencies`) of `modules`. A module outside the group
/// is followed through, and the nodes found past it are marked as reached through another chunk.
fn collect_imported_nodes(
  node: u32,
  modules: &FxHashSet<ModuleIdx>,
  node_by_module: &FxHashMap<ModuleIdx, u32>,
  ctx: &RequestMergeContext<'_>,
) -> FxHashMap<u32, bool> {
  let metas = &ctx.link_output.metas;
  let mut imports: FxHashMap<u32, bool> = FxHashMap::default();
  let mut visited_outside: FxHashSet<ModuleIdx> = FxHashSet::default();
  let mut pending: VecDeque<(ModuleIdx, bool)> = modules
    .iter()
    .flat_map(|module_idx| metas[*module_idx].load_dependencies.iter().map(|dep| (*dep, false)))
    .collect();
  while let Some((module_idx, via_outside)) = pending.pop_front() {
    if let Some(&imported) = node_by_module.get(&module_idx) {
      if imported != node {
        let through_other_chunk = imports.entry(imported).or_insert(false);
        *through_other_chunk |= via_outside;
      }
      continue;
    }
    if !metas[module_idx].is_included || !visited_outside.insert(module_idx) {
      continue;
    }
    pending.extend(metas[module_idx].load_dependencies.iter().map(|dep| (*dep, true)));
  }
  imports
}

/// Returns, for each module, the user-defined entries that load it: those from whose module it
/// can be reached through kept static imports (`load_dependencies`) and dynamic imports whose
/// statement tree shaking kept. Bits are positions in the list of user-defined entries.
pub(super) fn compute_module_loaders(
  link_output: &LinkStageOutput,
  flattened_entries: &[&EntryPoint],
) -> IndexVec<ModuleIdx, BitSet> {
  let metas = &link_output.metas;
  let mut dynamic_imports: FxHashMap<ModuleIdx, Vec<ModuleIdx>> = FxHashMap::default();
  for entry in flattened_entries {
    if entry.kind.is_user_defined() {
      continue;
    }
    for (importer, stmt_idx, _, _) in &entry.related_stmt_infos {
      if metas[*importer].is_included && metas[*importer].stmt_info_included.has_bit(*stmt_idx) {
        dynamic_imports.entry(*importer).or_default().push(entry.idx);
      }
    }
  }
  let user_entries: Vec<ModuleIdx> = flattened_entries
    .iter()
    .filter(|entry| entry.kind.is_user_defined())
    .map(|entry| entry.idx)
    .collect();
  let loaders_len: u32 = user_entries.len().try_into().expect("Too many entries");
  let mut loaders =
    IndexVec::from_vec(vec![BitSet::new(loaders_len); link_output.module_table.modules.len()]);
  for (loader, entry_module) in user_entries.iter().enumerate() {
    let loader: u32 = loader.try_into().expect("Too many entries");
    let mut pending = vec![*entry_module];
    while let Some(module_idx) = pending.pop() {
      if !matches!(link_output.module_table[module_idx], Module::Normal(_))
        || !metas[module_idx].is_included
        || !loaders[module_idx].set_bit(loader)
      {
        continue;
      }
      pending.extend(metas[module_idx].load_dependencies.iter().copied());
      pending.extend(dynamic_imports.get(&module_idx).into_iter().flatten().copied());
    }
  }
  loaders
}

/// A dense set of small integers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Bits(Vec<u64>);

impl Bits {
  fn new(len: usize) -> Self {
    Self(vec![0; len.div_ceil(64)])
  }

  fn has(&self, bit: u32) -> bool {
    self.0[bit as usize / 64] & (1 << (bit % 64)) != 0
  }

  fn set(&mut self, bit: u32) {
    self.0[bit as usize / 64] |= 1 << (bit % 64);
  }

  fn clear(&mut self, bit: u32) {
    self.0[bit as usize / 64] &= !(1 << (bit % 64));
  }

  fn union_with(&mut self, other: &Self) {
    for (word, other_word) in self.0.iter_mut().zip(&other.0) {
      *word |= other_word;
    }
  }

  fn ones(&self) -> impl Iterator<Item = u32> + '_ {
    words_ones(self.0.iter().copied())
  }

  /// Counts the bits set in `self` and in `other`.
  fn count_and(&self, other: &Self) -> u32 {
    self.0.iter().zip(&other.0).map(|(word, other_word)| (word & other_word).count_ones()).sum()
  }

  /// Counts the bits set in `self` and not in `other`.
  fn count_and_not(&self, other: &Self) -> u32 {
    self.0.iter().zip(&other.0).map(|(word, other_word)| (word & !other_word).count_ones()).sum()
  }
}

/// Returns the positions of the bits set in `words`.
fn words_ones(words: impl Iterator<Item = u64>) -> impl Iterator<Item = u32> {
  words.enumerate().flat_map(|(index, mut word)| {
    let base = u32::try_from(index * 64).expect("Too many bits");
    std::iter::from_fn(move || {
      if word == 0 {
        return None;
      }
      let bit = word.trailing_zeros();
      word &= word - 1;
      Some(base + bit)
    })
  })
}

/// A merge cost, ordered with `f64::total_cmp`.
#[derive(Clone, Copy, Debug)]
struct Cost(f64);

impl PartialEq for Cost {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == Ordering::Equal
  }
}

impl Eq for Cost {}

impl PartialOrd for Cost {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Cost {
  fn cmp(&self, other: &Self) -> Ordering {
    self.0.total_cmp(&other.0)
  }
}

/// A pair in the queue. `cost` is the exact cost when `exact` is set and a lower bound otherwise.
/// The entry is stale once either node's version moves past the one recorded here.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Candidate {
  cost: Cost,
  host: u32,
  absorbed: u32,
  exact: bool,
  host_version: u32,
  absorbed_version: u32,
}

/// The merge's state between two merges.
struct Merger {
  max_added_bytes_per_saved_request: f64,
  included_bytes: Vec<f64>,
  loaders: Vec<u32>,
  mergeable: Vec<bool>,
  alive: Vec<bool>,
  /// Moves whenever something a pair's cost reads may have changed for the node.
  version: Vec<u32>,
  imports: Vec<FxHashMap<u32, bool>>,
  importers: Vec<FxHashSet<u32>>,
  /// The node and every node it imports, transitively.
  closure: Vec<Bits>,
  /// For each load, the nodes it downloads: its seeds' closures.
  loaded: Vec<Bits>,
  /// For each node, the loads that download it.
  loaded_by: Vec<Bits>,
  queue: BinaryHeap<Reverse<Candidate>>,
}

/// Returns the merges to make, in order, each as `(host, absorbed)` node numbers.
///
/// Each step makes the allowed merge with the lowest cost, ties broken by `(host, absorbed)`. The
/// queue holds every pair that may merge, keyed by a lower bound until the pair reaches the front
/// and then by its exact cost. After a merge, only pairs with a node whose closure, loads or
/// bytes changed are priced again; every other pair's cost reads data the merge did not change.
fn plan_merges(
  nodes: &[NodeInput],
  loads: &[Vec<u32>],
  max_added_bytes_per_saved_request: f64,
) -> Vec<(u32, u32)> {
  let mut merger = Merger::new(nodes, loads, max_added_bytes_per_saved_request);
  let mut merges = Vec::new();
  while let Some(Reverse(candidate)) = merger.queue.pop() {
    let Candidate { host, absorbed, exact, host_version, absorbed_version, .. } = candidate;
    if !merger.alive[host as usize]
      || !merger.alive[absorbed as usize]
      || merger.version[host as usize] != host_version
      || merger.version[absorbed as usize] != absorbed_version
    {
      continue;
    }
    if !exact {
      if let Some(cost) = merger.exact_cost(host, absorbed)
        && cost <= max_added_bytes_per_saved_request
      {
        merger.queue.push(Reverse(Candidate { cost: Cost(cost), exact: true, ..candidate }));
      }
      continue;
    }
    // A refused pair returns to the queue when either node changes, which is the only way the
    // verdict can change.
    if merge_makes_cycle(&merger.imports, host, absorbed) {
      continue;
    }
    merger.merge(host, absorbed);
    merges.push((host, absorbed));
  }
  merges
}

impl Merger {
  fn new(nodes: &[NodeInput], loads: &[Vec<u32>], max_added_bytes_per_saved_request: f64) -> Self {
    let nodes_len = nodes.len();
    let mut importers = vec![FxHashSet::default(); nodes_len];
    for (node, input) in nodes.iter().enumerate() {
      for imported in input.imports.keys() {
        importers[*imported as usize].insert(u32::try_from(node).expect("Too many subgroups"));
      }
    }
    let imports: Vec<FxHashMap<u32, bool>> =
      nodes.iter().map(|node| node.imports.clone()).collect();
    let closure: Vec<Bits> = (0..nodes_len)
      .map(|node| {
        let mut closure = Bits::new(nodes_len);
        let mut pending = vec![u32::try_from(node).expect("Too many subgroups")];
        while let Some(node) = pending.pop() {
          if !closure.has(node) {
            closure.set(node);
            pending.extend(imports[node as usize].keys().copied());
          }
        }
        closure
      })
      .collect();
    let loaded: Vec<Bits> = loads
      .iter()
      .map(|seeds| {
        let mut loaded = Bits::new(nodes_len);
        for seed in seeds {
          loaded.union_with(&closure[*seed as usize]);
        }
        loaded
      })
      .collect();
    let mut loaded_by = vec![Bits::new(loads.len()); nodes_len];
    for (load, nodes) in loaded.iter().enumerate() {
      for node in nodes.ones() {
        loaded_by[node as usize].set(u32::try_from(load).expect("Too many loads"));
      }
    }
    let mut merger = Self {
      max_added_bytes_per_saved_request,
      included_bytes: nodes.iter().map(|node| node.included_bytes).collect(),
      loaders: nodes.iter().map(|node| node.loaders).collect(),
      mergeable: nodes.iter().map(|node| node.mergeable).collect(),
      alive: vec![true; nodes_len],
      version: vec![0; nodes_len],
      imports,
      importers,
      closure,
      loaded,
      loaded_by,
      queue: BinaryHeap::new(),
    };
    for node in 0..nodes_len {
      let node = u32::try_from(node).expect("Too many subgroups");
      merger.queue_pairs(node, |partner| partner > node);
    }
    merger
  }

  /// Queues every pair of `node` and a partner `include` accepts that may merge, keyed by a lower
  /// bound of its cost. A partner may merge when it is alive, has the same loaders, has no
  /// top-level await, and some load downloads both.
  fn queue_pairs(&mut self, node: u32, include: impl Fn(u32) -> bool) {
    if !self.mergeable[node as usize] {
      return;
    }
    let mut partners = Bits::new(self.alive.len());
    for load in self.loaded_by[node as usize].ones() {
      partners.union_with(&self.loaded[load as usize]);
    }
    for partner in partners.ones() {
      if partner == node
        || !self.alive[partner as usize]
        || !self.mergeable[partner as usize]
        || self.loaders[partner as usize] != self.loaders[node as usize]
        || !include(partner)
      {
        continue;
      }
      let (host, absorbed) = (node.min(partner), node.max(partner));
      if let Some(cost) = self.lower_bound_cost(host, absorbed)
        && cost <= self.max_added_bytes_per_saved_request
      {
        self.queue.push(Reverse(Candidate {
          cost: Cost(cost),
          host,
          absorbed,
          exact: false,
          host_version: self.version[host as usize],
          absorbed_version: self.version[absorbed as usize],
        }));
      }
    }
  }

  /// Returns a lower bound of the pair's cost: each load that downloads one node gains at least
  /// the other node, and the requests the merge adds are not subtracted. `None` when no load
  /// downloads both.
  fn lower_bound_cost(&self, host: u32, absorbed: u32) -> Option<f64> {
    let host_loads = &self.loaded_by[host as usize];
    let absorbed_loads = &self.loaded_by[absorbed as usize];
    let saved_requests = host_loads.count_and(absorbed_loads);
    if saved_requests == 0 {
      return None;
    }
    let added_bytes = self.included_bytes[absorbed as usize].mul_add(
      f64::from(host_loads.count_and_not(absorbed_loads)),
      self.included_bytes[host as usize] * f64::from(absorbed_loads.count_and_not(host_loads)),
    );
    Some(added_bytes / f64::from(saved_requests))
  }

  /// Returns the included bytes the merge adds over all loads per request it saves them, or
  /// `None` when it saves no request in total. A load that downloads one node and not the other
  /// gains the other node's closure, apart from what it already downloads.
  fn exact_cost(&self, host: u32, absorbed: u32) -> Option<f64> {
    let host_loads = &self.loaded_by[host as usize];
    let absorbed_loads = &self.loaded_by[absorbed as usize];
    let saved_requests = host_loads.count_and(absorbed_loads);
    let mut added_requests = 0_u32;
    let mut added_bytes = 0.0;
    let one_side_loads = words_ones(
      host_loads.0.iter().zip(&absorbed_loads.0).map(|(host, absorbed)| host ^ absorbed),
    );
    for load in one_side_loads {
      let other = if host_loads.has(load) { absorbed } else { host };
      let loaded = &self.loaded[load as usize];
      let added = self.closure[other as usize]
        .0
        .iter()
        .zip(&loaded.0)
        .map(|(closure_word, loaded_word)| closure_word & !loaded_word);
      let mut added_nodes = 0_u32;
      for node in words_ones(added) {
        added_bytes += self.included_bytes[node as usize];
        added_nodes += 1;
      }
      // `other` itself joins a chunk the load already downloads.
      added_requests += added_nodes - 1;
    }
    let net_saved_requests = saved_requests.checked_sub(added_requests).filter(|net| *net > 0)?;
    Some(added_bytes / f64::from(net_saved_requests))
  }

  /// Merges `absorbed` into `host`, updates closures and loads, and queues again every pair whose
  /// cost the merge may have changed.
  fn merge(&mut self, host: u32, absorbed: u32) {
    // Nodes that reach either one: their closures change.
    let mut dirty = self.ancestors([host, absorbed].into_iter());

    let absorbed_imports = std::mem::take(&mut self.imports[absorbed as usize]);
    for (imported, through_other_chunk) in absorbed_imports {
      self.importers[imported as usize].remove(&absorbed);
      if imported != host {
        self.importers[imported as usize].insert(host);
        *self.imports[host as usize].entry(imported).or_insert(false) |= through_other_chunk;
      }
    }
    let absorbed_importers = std::mem::take(&mut self.importers[absorbed as usize]);
    for importer in absorbed_importers {
      let through_other_chunk =
        self.imports[importer as usize].remove(&absorbed).expect("importer should import");
      if importer != host {
        *self.imports[importer as usize].entry(host).or_insert(false) |= through_other_chunk;
        self.importers[host as usize].insert(importer);
      }
    }
    self.imports[host as usize].remove(&host);
    self.importers[host as usize].remove(&host);
    self.included_bytes[host as usize] += self.included_bytes[absorbed as usize];
    self.alive[absorbed as usize] = false;

    let mut host_closure = std::mem::replace(&mut self.closure[absorbed as usize], Bits::new(0));
    host_closure.union_with(&self.closure[host as usize]);
    host_closure.clear(absorbed);
    host_closure.set(host);
    for node in dirty.ones() {
      if node != host && node != absorbed {
        let closure = &mut self.closure[node as usize];
        if closure.has(absorbed) {
          closure.clear(absorbed);
          closure.set(host);
        }
        closure.union_with(&host_closure);
      }
    }
    self.closure[host as usize] = host_closure;

    // Loads that downloaded either node now download the merged node's closure too.
    let mut affected_loads = self.loaded_by[host as usize].clone();
    affected_loads.union_with(&self.loaded_by[absorbed as usize]);
    self.loaded_by[host as usize] = affected_loads.clone();
    let mut added_to_some_load = Bits::new(self.alive.len());
    for load in affected_loads.ones() {
      let loaded = &mut self.loaded[load as usize];
      if loaded.has(absorbed) {
        loaded.clear(absorbed);
        loaded.set(host);
      }
      for (word, (closure_word, added_word)) in
        loaded.0.iter_mut().zip(self.closure[host as usize].0.iter().zip(&mut added_to_some_load.0))
      {
        let added = closure_word & !*word;
        *word |= added;
        *added_word |= added;
      }
    }
    for node in added_to_some_load.ones() {
      for load in affected_loads.ones() {
        if self.loaded[load as usize].has(node) {
          self.loaded_by[node as usize].set(load);
        }
      }
    }

    // Pairs with a node a load started downloading change their saved requests or their added
    // bytes, and pairs with a node that reaches one changes the bytes a load gains from it.
    dirty.union_with(&added_to_some_load);
    dirty.union_with(&self.ancestors(added_to_some_load.ones()));
    dirty.set(host);
    dirty.clear(absorbed);
    let dirty_nodes: Vec<u32> = dirty.ones().filter(|node| self.alive[*node as usize]).collect();
    for node in &dirty_nodes {
      self.version[*node as usize] += 1;
    }
    for node in dirty_nodes {
      self.queue_pairs(node, |partner| partner > node || !dirty.has(partner));
    }
  }

  /// Returns `nodes` and every node that imports one of them, transitively.
  fn ancestors(&self, nodes: impl Iterator<Item = u32>) -> Bits {
    let mut ancestors = Bits::new(self.alive.len());
    let mut pending: Vec<u32> = nodes.collect();
    while let Some(node) = pending.pop() {
      if !ancestors.has(node) {
        ancestors.set(node);
        pending.extend(self.importers[node as usize].iter().copied());
      }
    }
    ancestors
  }
}

/// Tells whether merging makes a chunk cycle: one node reaches the other through a third node or
/// through a module outside the group.
fn merge_makes_cycle(imports: &[FxHashMap<u32, bool>], host: u32, absorbed: u32) -> bool {
  let reaches_through_third = |from: u32, to: u32| {
    imports[from as usize].iter().any(|(&imported, &through_other_chunk)| {
      if imported == to { through_other_chunk } else { reaches(imports, imported, to, from) }
    })
  };
  reaches_through_third(host, absorbed) || reaches_through_third(absorbed, host)
}

/// Tells whether `to` is `from` or among the nodes `from` imports, transitively, without passing
/// through `skip`.
fn reaches(imports: &[FxHashMap<u32, bool>], from: u32, to: u32, skip: u32) -> bool {
  let mut visited = FxHashSet::default();
  let mut pending = vec![from];
  while let Some(node) = pending.pop() {
    if node == to {
      return true;
    }
    if node == skip || !visited.insert(node) {
      continue;
    }
    pending.extend(imports[node as usize].keys().copied());
  }
  false
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use rustc_hash::{FxHashMap, FxHashSet};

  use super::{NodeInput, merge_makes_cycle, plan_merges};

  const NO_MERGES: Vec<(u32, u32)> = Vec::new();

  fn node(included_bytes: f64, imports: &[(u32, bool)]) -> NodeInput {
    NodeInput {
      included_bytes,
      loaders: 0,
      mergeable: true,
      imports: imports.iter().copied().collect(),
    }
  }

  fn imports_of(nodes: &[NodeInput]) -> Vec<FxHashMap<u32, bool>> {
    nodes.iter().map(|node| node.imports.clone()).collect()
  }

  #[test]
  fn direct_import_does_not_make_a_cycle() {
    let nodes = [node(1.0, &[(1, false)]), node(1.0, &[])];
    assert!(!merge_makes_cycle(&imports_of(&nodes), 0, 1));
  }

  #[test]
  fn path_through_a_third_node_makes_a_cycle() {
    // 0 -> 2 -> 1: merging 0 and 1 leaves the merged chunk and 2 importing each other.
    let nodes = [node(1.0, &[(2, false)]), node(1.0, &[]), node(1.0, &[(1, false)])];
    let imports = imports_of(&nodes);
    assert!(merge_makes_cycle(&imports, 0, 1));
    assert!(merge_makes_cycle(&imports, 1, 0));
    assert!(!merge_makes_cycle(&imports, 0, 2));
  }

  #[test]
  fn path_through_another_chunk_makes_a_cycle() {
    // 0 reaches 1 only through a module outside the group.
    let nodes = [node(1.0, &[(1, true)]), node(1.0, &[])];
    assert!(merge_makes_cycle(&imports_of(&nodes), 0, 1));
  }

  #[test]
  fn cost_counts_saved_requests_and_added_bytes() {
    // Load A downloads 0 and 1; load B downloads only 1. Merging saves A one request and adds 0's
    // 10 bytes to B.
    let loads = [vec![0, 1], vec![1]];
    assert_eq!(plan_merges(&[node(10.0, &[]), node(30.0, &[])], &loads, 10.0), [(0, 1)]);
    assert_eq!(plan_merges(&[node(10.0, &[]), node(30.0, &[])], &loads, 9.0), NO_MERGES);
  }

  #[test]
  fn cost_subtracts_requests_the_merge_adds() {
    // 1 imports 2. Load A downloads 0, 1 and 2; load B downloads only 0. Merging 0 and 1 would
    // make B download 1 and 2, one request more, so it saves nothing in total; merging 1 and 2
    // saves A one request and costs nothing.
    let nodes = || vec![node(10.0, &[]), node(20.0, &[(2, false)]), node(5.0, &[])];
    let loads = [vec![0, 1], vec![0]];
    assert_eq!(plan_merges(&nodes(), &loads, 0.0), [(1, 2)]);
  }

  #[test]
  fn nodes_with_top_level_await_never_merge() {
    let loads = [vec![0, 1], vec![1]];
    let mut nodes = vec![node(10.0, &[]), node(30.0, &[])];
    nodes[1].mergeable = false;
    assert_eq!(plan_merges(&nodes, &loads, 100.0), NO_MERGES);
  }

  #[test]
  fn nodes_with_different_loaders_never_merge() {
    let loads = [vec![0, 1]];
    let mut nodes = vec![node(10.0, &[]), node(30.0, &[])];
    nodes[1].loaders = 1;
    assert_eq!(plan_merges(&nodes, &loads, 100.0), NO_MERGES);
  }

  /// A deterministic generator for random graphs.
  struct Rng(u64);

  impl Rng {
    fn below(&mut self, bound: u32) -> u32 {
      self.0 ^= self.0 << 13;
      self.0 ^= self.0 >> 7;
      self.0 ^= self.0 << 17;
      u32::try_from(self.0 % u64::from(bound)).unwrap()
    }
  }

  /// Returns nodes that mostly import nodes numbered after them, as entries' subgroups do, with a
  /// few imports back and a few through other chunks, and loads that each seed a few nodes.
  fn random_graph(
    rng: &mut Rng,
    nodes_len: u32,
    loads_len: u32,
    seeds_per_load: u32,
  ) -> (Vec<NodeInput>, Vec<Vec<u32>>) {
    let nodes = (0..nodes_len)
      .map(|node| {
        let mut imports = FxHashMap::default();
        for _ in 0..rng.below(4) {
          let imported = if rng.below(10) == 0 {
            rng.below(nodes_len)
          } else {
            node + 1 + rng.below((nodes_len - node).max(1))
          };
          if imported != node && imported < nodes_len {
            imports.insert(imported, rng.below(8) == 0);
          }
        }
        NodeInput {
          included_bytes: f64::from(1 + rng.below(2000)),
          loaders: u32::from(rng.below(12) == 0),
          mergeable: rng.below(30) != 0,
          imports,
        }
      })
      .collect();
    let loads = (0..loads_len)
      .map(|_| {
        let seeds: FxHashSet<u32> =
          (0..=rng.below(seeds_per_load)).map(|_| rng.below(nodes_len)).collect();
        seeds.into_iter().collect()
      })
      .collect();
    (nodes, loads)
  }

  /// The search before it was made incremental: every step prices every pair against every load
  /// from scratch.
  fn reference_plan(
    nodes: &[NodeInput],
    loads: &[Vec<u32>],
    max_added_bytes_per_saved_request: f64,
  ) -> Vec<(u32, u32)> {
    let mut imports = imports_of(nodes);
    let mut bytes: Vec<f64> = nodes.iter().map(|node| node.included_bytes).collect();
    let mut alive = vec![true; nodes.len()];
    let mut seeds: Vec<FxHashSet<u32>> =
      loads.iter().map(|seeds| seeds.iter().copied().collect()).collect();
    let collect =
      |imports: &[FxHashMap<u32, bool>], seeds: &FxHashSet<u32>, merge: Option<(u32, u32)>| {
        let mut loaded = FxHashSet::default();
        let mut pending: Vec<u32> = seeds.iter().copied().collect();
        while let Some(node) = pending.pop() {
          let node = match merge {
            Some((host, absorbed)) if node == absorbed => host,
            _ => node,
          };
          if loaded.insert(node) {
            pending.extend(imports[node as usize].keys().copied());
            if let Some((host, absorbed)) = merge
              && node == host
            {
              pending.extend(imports[absorbed as usize].keys().copied());
            }
          }
        }
        loaded
      };
    let mut merges = Vec::new();
    loop {
      let loaded: Vec<FxHashSet<u32>> =
        seeds.iter().map(|seeds| collect(&imports, seeds, None)).collect();
      let mut cheapest: Option<(f64, u32, u32)> = None;
      for host in 0..u32::try_from(nodes.len()).unwrap() {
        for absorbed in host + 1..u32::try_from(nodes.len()).unwrap() {
          let (h, a) = (host as usize, absorbed as usize);
          if !alive[h]
            || !alive[a]
            || !nodes[h].mergeable
            || !nodes[a].mergeable
            || nodes[h].loaders != nodes[a].loaders
          {
            continue;
          }
          let mut saved = 0_i64;
          let mut added_requests = 0_i64;
          let mut added_bytes = 0.0;
          for (load, loaded) in loaded.iter().enumerate() {
            let (has_host, has_absorbed) = (loaded.contains(&host), loaded.contains(&absorbed));
            if has_host && has_absorbed {
              saved += 1;
            } else if has_host || has_absorbed {
              let after = collect(&imports, &seeds[load], Some((host, absorbed)));
              let bytes_of = |node: &u32| {
                if *node == host { bytes[h] + bytes[a] } else { bytes[*node as usize] }
              };
              added_bytes += after.iter().map(bytes_of).sum::<f64>()
                - loaded.iter().map(|node| bytes[*node as usize]).sum::<f64>();
              added_requests +=
                i64::try_from(after.len()).unwrap() - i64::try_from(loaded.len()).unwrap();
            }
          }
          let net = saved - added_requests;
          if saved == 0 || net <= 0 {
            continue;
          }
          #[expect(clippy::cast_precision_loss)]
          let cost = added_bytes / net as f64;
          if cost > max_added_bytes_per_saved_request
            || cheapest.is_some_and(|(cheapest, _, _)| cost >= cheapest)
            || merge_makes_cycle(&imports, host, absorbed)
          {
            continue;
          }
          cheapest = Some((cost, host, absorbed));
        }
      }
      let Some((_, host, absorbed)) = cheapest else {
        return merges;
      };
      merges.push((host, absorbed));
      alive[absorbed as usize] = false;
      bytes[host as usize] += bytes[absorbed as usize];
      let absorbed_imports = std::mem::take(&mut imports[absorbed as usize]);
      for node_imports in &mut imports {
        if let Some(through_other_chunk) = node_imports.remove(&absorbed) {
          *node_imports.entry(host).or_insert(false) |= through_other_chunk;
        }
      }
      for (imported, through_other_chunk) in absorbed_imports {
        *imports[host as usize].entry(imported).or_insert(false) |= through_other_chunk;
      }
      imports[host as usize].remove(&host);
      for seeds in &mut seeds {
        if seeds.remove(&absorbed) {
          seeds.insert(host);
        }
      }
    }
  }

  #[test]
  fn incremental_search_matches_the_exhaustive_search() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for case in 0..300 {
      let nodes_len = 2 + rng.below(40);
      let loads_len = 1 + rng.below(30);
      let (nodes, loads) = random_graph(&mut rng, nodes_len, loads_len, 6);
      let max = f64::from(rng.below(6000));
      let expected = reference_plan(&nodes, &loads, max);
      assert_eq!(plan_merges(&nodes, &loads, max), expected, "case {case}");
    }
  }

  /// Returns a graph shaped like an app's: 1,000 route subgroups that each import a few of 1,200
  /// component subgroups, which import a few of 800 utility subgroups, and 2,000 loads of single
  /// dynamic entries, each downloading one to three route or component subgroups.
  fn app_like_graph(rng: &mut Rng) -> (Vec<NodeInput>, Vec<Vec<u32>>) {
    const ROUTES: u32 = 1000;
    const COMPONENTS: u32 = 1200;
    const UTILITIES: u32 = 800;
    let nodes = (0..ROUTES + COMPONENTS + UTILITIES)
      .map(|node| {
        let (first, len, count) = if node < ROUTES {
          (ROUTES, COMPONENTS, 1 + rng.below(4))
        } else if node < ROUTES + COMPONENTS {
          (ROUTES + COMPONENTS, UTILITIES, rng.below(4))
        } else {
          (ROUTES + COMPONENTS, UTILITIES, rng.below(2))
        };
        let imports = (0..count)
          .map(|_| (first + rng.below(len), rng.below(20) == 0))
          .filter(|(imported, _)| *imported != node)
          .collect();
        NodeInput {
          included_bytes: f64::from(1 + rng.below(4000)),
          loaders: u32::from(rng.below(10) == 0),
          mergeable: true,
          imports,
        }
      })
      .collect();
    let loads = (0..2000)
      .map(|_| (0..=rng.below(3)).map(|_| rng.below(ROUTES + COMPONENTS)).collect())
      .collect();
    (nodes, loads)
  }

  /// Times the search on [`app_like_graph`] plus ten page loads that each download 20 route or
  /// component subgroups. Prints the time with `--nocapture`.
  #[test]
  #[expect(clippy::print_stderr)]
  fn thousands_of_dynamic_entries() {
    let mut rng = Rng(42);
    let (nodes, mut loads) = app_like_graph(&mut rng);
    loads.extend((0..10).map(|_| (0..20).map(|_| rng.below(2200)).collect()));
    let started = Instant::now();
    let merges = plan_merges(&nodes, &loads, 8000.0);
    eprintln!("{} merges in {:?}", merges.len(), started.elapsed());
    assert_ne!(merges.len(), 0);
  }
}
