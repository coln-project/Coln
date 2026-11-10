// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::{BTreeSet, HashMap};

use crate::commit::{CommitHash, DataCommit, RootCommit};

/// A DAG of commits, tracking parent relationships and the current heads.
///
/// `heads` are commits that no other known commit depends on yet — the tips of
/// the DAG.  Every new transaction uses the current heads as its deps; after
/// the commit is recorded the heads are updated via [`CommitGraph::add_commit`].
#[derive(Debug, Clone)]
pub struct CommitGraph {
    root: RootCommit,
    commits: HashMap<CommitHash, DataCommit>,
    children: HashMap<CommitHash, BTreeSet<CommitHash>>,
    /// Commits that no other known commit depends on yet.
    heads: BTreeSet<CommitHash>,
}

impl CommitGraph {
    pub fn new(root: RootCommit) -> Self {
        let heads = BTreeSet::from([root.hash()]);
        Self {
            root,
            commits: HashMap::new(),
            children: HashMap::new(),
            heads,
        }
    }

    /// The current frontier: commits that nothing else (yet) depends on.
    /// Sorted for deterministic iteration.
    pub fn heads(&self) -> impl Iterator<Item = &CommitHash> {
        self.heads.iter()
    }

    /// Returns `true` if `hash` has already been recorded.
    pub fn contains(&self, hash: &CommitHash) -> bool {
        *hash == self.root.hash() || self.commits.contains_key(hash)
    }

    pub fn get_data(&self, hash: &CommitHash) -> Option<&DataCommit> {
        self.commits.get(hash)
    }

    /// Direct parents (deps) of `hash`, or `None` if the commit is unknown.
    pub fn parents_of(&self, hash: &CommitHash) -> Option<&[CommitHash]> {
        if *hash == self.root.hash() {
            return Some(&[]);
        }
        self.get_data(hash).map(|c| c.deps.as_slice())
    }

    /// Record a new commit.  Its `deps` are removed from heads (they are no
    /// longer tips) and `hash` is added as the new head.
    ///
    /// Inserting the same hash twice is a no-op.
    pub fn add_commit(&mut self, commit: DataCommit) {
        let hash = commit.hash();
        if self.commits.contains_key(&hash) {
            return;
        }

        for dep in &commit.deps {
            self.heads.remove(dep);
            self.children.entry(*dep).or_default().insert(hash);
        }
        self.heads.insert(hash);
        self.commits.insert(hash, commit);
    }

    /// Data commits in dependency order. The root is available separately.
    pub fn iter_topological(&self) -> impl Iterator<Item = &DataCommit> {
        let root_hash = self.root.hash();
        let mut remaining_parents: HashMap<CommitHash, usize> = self
            .commits
            .iter()
            .map(|(hash, commit)| {
                let count = commit.deps.iter().filter(|dep| **dep != root_hash).count();
                (*hash, count)
            })
            .collect();

        let mut ready: BTreeSet<CommitHash> = remaining_parents
            .iter()
            .filter_map(|(hash, count)| (*count == 0).then_some(*hash))
            .collect();

        let mut ordered = Vec::with_capacity(self.commits.len());

        while let Some(hash) = ready.pop_first() {
            ordered.push(hash);

            if let Some(children) = self.children.get(&hash) {
                for child in children {
                    let Some(count) = remaining_parents.get_mut(child) else {
                        continue;
                    };

                    *count -= 1;
                    if *count == 0 {
                        ready.insert(*child);
                    }
                }
            }
        }

        ordered
            .into_iter()
            .filter_map(|hash| self.commits.get(&hash))
    }

    pub fn root_commit(&self) -> &RootCommit {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::hash::HASH_SIZE;
    use rstest::{fixture, rstest};

    use super::*;
    use crate::commit::{
        author::Author,
        wire::{CommitData, RootCommitData},
    };
    use crate::test_utils::non_empty_root_commit_data;

    fn h(n: u8) -> CommitHash {
        CommitHash([n; HASH_SIZE])
    }

    fn c(n: u8, deps: Vec<CommitHash>) -> DataCommit {
        DataCommit::from_commit_data(
            CommitData::new(deps, Author::foo(), n as i64, None, vec![]),
            |_| None,
        )
        .expect("build commit")
    }

    #[fixture]
    fn root(non_empty_root_commit_data: RootCommitData) -> RootCommit {
        RootCommit::from_root_data(&non_empty_root_commit_data).expect("build root")
    }

    #[rstest]
    fn new_graph_has_root_as_sole_head(root: RootCommit) {
        let g = CommitGraph::new(root);
        let hash = g.root_commit().hash();
        assert_eq!(g.heads().copied().collect::<Vec<_>>(), vec![hash]);
        assert!(g.contains(&hash));
        assert!(g.get_data(&hash).is_none());
        assert_eq!(g.parents_of(&hash), Some([].as_slice()));
        assert_eq!(g.iter_topological().count(), 0);
    }

    #[rstest]
    fn first_commit_becomes_head(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        let first_hash = first.hash();
        g.add_commit(first);
        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&first_hash]);
    }

    #[rstest]
    fn second_commit_replaces_first_as_head(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        let first_hash = first.hash();
        g.add_commit(first);

        let second = c(2, vec![first_hash]);
        let second_hash = second.hash();
        g.add_commit(second);

        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&second_hash]);
    }

    #[rstest]
    fn two_divergent_commits_both_heads(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        let first_hash = first.hash();
        g.add_commit(first);

        let second = c(2, vec![first_hash]);
        let second_hash = second.hash();
        g.add_commit(second);

        let third = c(3, vec![first_hash]);
        let third_hash = third.hash();
        g.add_commit(third);

        let heads: Vec<_> = g.heads().collect();
        assert_eq!(heads.len(), 2);
        assert!(heads.contains(&&second_hash));
        assert!(heads.contains(&&third_hash));
    }

    #[rstest]
    fn merge_commit_reconciles_two_heads(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        let first_hash = first.hash();
        g.add_commit(first);

        let second = c(2, vec![first_hash]);
        let second_hash = second.hash();
        g.add_commit(second);

        let third = c(3, vec![first_hash]);
        let third_hash = third.hash();
        g.add_commit(third);

        let fourth = c(4, vec![second_hash, third_hash]);
        let fourth_hash = fourth.hash();
        g.add_commit(fourth);

        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&fourth_hash]);
    }

    #[rstest]
    fn duplicate_insert_is_noop(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        g.add_commit(first.clone());
        g.add_commit(first); // second insert ignored
        assert_eq!(g.heads().count(), 1);
    }

    #[rstest]
    fn parents_of_known_commit(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        let first_hash = first.hash();
        g.add_commit(first);

        let second = c(2, vec![first_hash]);
        let second_hash = second.hash();
        g.add_commit(second);

        let expected = vec![first_hash];
        assert_eq!(g.parents_of(&second_hash), Some(expected.as_slice()));
    }

    #[rstest]
    fn parents_of_unknown_commit_is_none(root: RootCommit) {
        let g = CommitGraph::new(root);
        assert_eq!(g.parents_of(&h(99)), None);
    }

    #[rstest]
    fn iter_topological_returns_parents_before_children(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        let first_hash = first.hash();
        g.add_commit(first);

        let second = c(2, vec![first_hash]);
        let second_hash = second.hash();
        g.add_commit(second);

        let third = c(3, vec![first_hash]);
        let third_hash = third.hash();
        g.add_commit(third);

        let fourth = c(4, vec![second_hash, third_hash]);
        let fourth_hash = fourth.hash();
        g.add_commit(fourth);

        let order: Vec<_> = g.iter_topological().map(DataCommit::hash).collect();

        assert_eq!(order.len(), 4);
        assert!(
            order.iter().position(|hash| *hash == first_hash).unwrap()
                < order.iter().position(|hash| *hash == second_hash).unwrap()
        );
        assert!(
            order.iter().position(|hash| *hash == first_hash).unwrap()
                < order.iter().position(|hash| *hash == third_hash).unwrap()
        );
        assert!(
            order.iter().position(|hash| *hash == second_hash).unwrap()
                < order.iter().position(|hash| *hash == fourth_hash).unwrap()
        );
        assert!(
            order.iter().position(|hash| *hash == third_hash).unwrap()
                < order.iter().position(|hash| *hash == fourth_hash).unwrap()
        );
    }

    #[rstest]
    fn root_commit_returns_constructor_root(root: RootCommit) {
        let hash = root.hash();
        let g = CommitGraph::new(root);
        assert_eq!(g.root_commit().hash(), hash);
    }

    #[rstest]
    fn topological_order_handles_root_and_data_dependencies(root: RootCommit) {
        let root_hash = root.hash();
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![root_hash]);
        let first_hash = first.hash();
        let second = c(2, vec![root_hash, first_hash]);
        let second_hash = second.hash();
        g.add_commit(first);
        g.add_commit(second);

        assert_eq!(
            g.iter_topological()
                .map(DataCommit::hash)
                .collect::<Vec<_>>(),
            vec![first_hash, second_hash]
        );
        assert_eq!(g.heads().copied().collect::<Vec<_>>(), vec![second_hash]);
        assert!(g.contains(&root_hash));
    }

    #[rstest]
    fn contains_known_and_unknown(root: RootCommit) {
        let mut g = CommitGraph::new(root);
        let first = c(1, vec![g.root_commit().hash()]);
        let first_hash = first.hash();
        g.add_commit(first);

        assert!(g.contains(&first_hash));
        assert!(!g.contains(&h(2)));
    }
}
