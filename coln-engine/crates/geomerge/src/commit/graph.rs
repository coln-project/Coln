use std::collections::{BTreeSet, HashMap};

use crate::commit::CommitHash;

/// A DAG of commits, tracking parent relationships and the current heads.
///
/// `heads` are commits that no other known commit depends on yet — the tips of
/// the DAG.  Every new transaction uses the current heads as its deps; after
/// the commit is recorded the heads are updated via [`CommitGraph::add_commit`].
#[derive(Debug, Clone, Default)]
pub struct CommitGraph {
    /// Maps each known commit to its direct parent hashes (its deps).
    parents: HashMap<CommitHash, Vec<CommitHash>>,
    /// Commits that no other known commit depends on yet.
    heads: BTreeSet<CommitHash>,
}

impl CommitGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current frontier: commits that nothing else (yet) depends on.
    /// Sorted for deterministic iteration.
    pub fn heads(&self) -> impl Iterator<Item = &CommitHash> {
        self.heads.iter()
    }

    /// Returns `true` if `hash` has already been recorded.
    pub fn contains(&self, hash: &CommitHash) -> bool {
        self.parents.contains_key(hash)
    }

    /// Direct parents (deps) of `hash`, or `None` if the commit is unknown.
    pub fn parents_of(&self, hash: &CommitHash) -> Option<&[CommitHash]> {
        self.parents.get(hash).map(Vec::as_slice)
    }

    /// Record a new commit.  Its `deps` are removed from heads (they are no
    /// longer tips) and `hash` is added as the new head.
    ///
    /// Inserting the same hash twice is a no-op.
    pub fn add_commit(&mut self, hash: CommitHash, deps: Vec<CommitHash>) {
        if self.parents.contains_key(&hash) {
            return;
        }
        for dep in &deps {
            self.heads.remove(dep);
        }
        self.heads.insert(hash);
        self.parents.insert(hash, deps);
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::commit::hash::HASH_SIZE;

    fn h(n: u8) -> CommitHash {
        CommitHash([n; HASH_SIZE])
    }

    #[test]
    fn empty_graph_has_no_heads() {
        let g = CommitGraph::new();
        assert_eq!(g.heads().count(), 0);
    }

    #[test]
    fn first_commit_becomes_head() {
        let mut g = CommitGraph::new();
        g.add_commit(h(1), vec![]);
        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&h(1)]);
    }

    #[test]
    fn second_commit_replaces_first_as_head() {
        let mut g = CommitGraph::new();
        g.add_commit(h(1), vec![]);
        g.add_commit(h(2), vec![h(1)]);
        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&h(2)]);
    }

    #[test]
    fn two_divergent_commits_both_heads() {
        let mut g = CommitGraph::new();
        g.add_commit(h(1), vec![]);
        g.add_commit(h(2), vec![h(1)]);
        g.add_commit(h(3), vec![h(1)]);
        let heads: Vec<_> = g.heads().collect();
        assert_eq!(heads.len(), 2);
        assert!(heads.contains(&&h(2)));
        assert!(heads.contains(&&h(3)));
    }

    #[test]
    fn merge_commit_reconciles_two_heads() {
        let mut g = CommitGraph::new();
        g.add_commit(h(1), vec![]);
        g.add_commit(h(2), vec![h(1)]);
        g.add_commit(h(3), vec![h(1)]);
        g.add_commit(h(4), vec![h(2), h(3)]);
        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&h(4)]);
    }

    #[test]
    fn duplicate_insert_is_noop() {
        let mut g = CommitGraph::new();
        g.add_commit(h(1), vec![]);
        g.add_commit(h(1), vec![]); // second insert ignored
        assert_eq!(g.heads().count(), 1);
    }

    #[test]
    fn parents_of_known_commit() {
        let mut g = CommitGraph::new();
        g.add_commit(h(1), vec![]);
        g.add_commit(h(2), vec![h(1)]);
        assert_eq!(g.parents_of(&h(2)), Some([h(1)].as_slice()));
    }

    #[test]
    fn parents_of_unknown_commit_is_none() {
        let g = CommitGraph::new();
        assert_eq!(g.parents_of(&h(99)), None);
    }

    #[test]
    fn contains_known_and_unknown() {
        let mut g = CommitGraph::new();
        g.add_commit(h(1), vec![]);
        assert!(g.contains(&h(1)));
        assert!(!g.contains(&h(2)));
    }
}
