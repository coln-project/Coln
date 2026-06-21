// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::{BTreeSet, HashMap};

use crate::commit::{Commit, CommitHash};

/// A DAG of commits, tracking parent relationships and the current heads.
///
/// `heads` are commits that no other known commit depends on yet — the tips of
/// the DAG.  Every new transaction uses the current heads as its deps; after
/// the commit is recorded the heads are updated via [`CommitGraph::add_commit`].
#[derive(Debug, Clone, Default)]
pub struct CommitGraph {
    commits: HashMap<CommitHash, Commit<'static>>,
    children: HashMap<CommitHash, BTreeSet<CommitHash>>,
    /// Commits that no other known commit depends on yet.
    heads: BTreeSet<CommitHash>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommitGraphError {
    #[error("commit graph has no root commit")]
    MissingRoot,
    #[error("commit graph has multiple root commits")]
    MultipleRoots,
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
        self.commits.contains_key(hash)
    }

    pub fn get(&self, hash: &CommitHash) -> Option<&Commit<'static>> {
        self.commits.get(hash)
    }

    /// Direct parents (deps) of `hash`, or `None` if the commit is unknown.
    pub fn parents_of(&self, hash: &CommitHash) -> Option<&[CommitHash]> {
        self.get(hash).map(|c| c.deps.as_slice())
    }

    /// Record a new commit.  Its `deps` are removed from heads (they are no
    /// longer tips) and `hash` is added as the new head.
    ///
    /// Inserting the same hash twice is a no-op.
    pub fn add_commit(&mut self, commit: Commit<'static>) {
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

    pub fn iter_topological(&self) -> impl Iterator<Item = &Commit<'static>> {
        let mut remaining_parents: HashMap<CommitHash, usize> = self
            .commits
            .iter()
            .map(|(hash, commit)| (*hash, commit.deps.len()))
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

    // Currently this is O(commits), but this function is not on a hot path yet
    pub fn root_commit(&self) -> Result<&Commit<'static>, CommitGraphError> {
        let mut roots = self.commits.values().filter(|cmt| cmt.is_root());

        let root = roots.next().ok_or(CommitGraphError::MissingRoot)?;
        if roots.next().is_some() {
            return Err(CommitGraphError::MultipleRoots);
        }

        Ok(root)
    }
}

#[cfg(test)]
mod tests {

    use coln_flir_rs::ir::EntityVariant;

    use super::*;
    use crate::commit::author::Author;
    use crate::commit::hash::HASH_SIZE;
    use crate::commit::wire::root::{RootCommitData, RootTableEntry};
    use crate::ir::Schema;

    fn h(n: u8) -> CommitHash {
        CommitHash([n; HASH_SIZE])
    }

    fn c(n: u8, deps: Vec<CommitHash>) -> Commit<'static> {
        Commit::from_commit_data(
            crate::commit::wire::CommitData::new(deps, Author::foo(), n as i64, None, vec![]),
            |_| None,
        )
        .expect("build commit")
    }

    fn root() -> Commit<'static> {
        root_with_table(0)
    }

    fn root_with_table(oid: u64) -> Commit<'static> {
        Commit::from_root_data(&RootCommitData {
            tables: vec![RootTableEntry {
                path: format!("T{oid}"),
                oid,
                schema: Schema {
                    entity_variant: EntityVariant::Table,
                    columns: vec![],
                    primary_key: None,
                },
            }],
            laws: vec![],
        })
        .expect("build root commit")
    }

    #[test]
    fn empty_graph_has_no_heads() {
        let g = CommitGraph::new();
        assert_eq!(g.heads().count(), 0);
    }

    #[test]
    fn first_commit_becomes_head() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
        let first_hash = first.hash();
        g.add_commit(first);
        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&first_hash]);
    }

    #[test]
    fn second_commit_replaces_first_as_head() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
        let first_hash = first.hash();
        g.add_commit(first);

        let second = c(2, vec![first_hash]);
        let second_hash = second.hash();
        g.add_commit(second);

        assert_eq!(g.heads().collect::<Vec<_>>(), vec![&second_hash]);
    }

    #[test]
    fn two_divergent_commits_both_heads() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
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

    #[test]
    fn merge_commit_reconciles_two_heads() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
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

    #[test]
    fn duplicate_insert_is_noop() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
        g.add_commit(first.clone());
        g.add_commit(first); // second insert ignored
        assert_eq!(g.heads().count(), 1);
    }

    #[test]
    fn parents_of_known_commit() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
        let first_hash = first.hash();
        g.add_commit(first);

        let second = c(2, vec![first_hash]);
        let second_hash = second.hash();
        g.add_commit(second);

        let expected = vec![first_hash];
        assert_eq!(g.parents_of(&second_hash), Some(expected.as_slice()));
    }

    #[test]
    fn parents_of_unknown_commit_is_none() {
        let g = CommitGraph::new();
        assert_eq!(g.parents_of(&h(99)), None);
    }

    #[test]
    fn iter_topological_returns_parents_before_children() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
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

        let order: Vec<_> = g.iter_topological().map(Commit::hash).collect();

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

    #[test]
    fn root_commit_rejects_missing_root() {
        let g = CommitGraph::new();
        assert_eq!(g.root_commit().unwrap_err(), CommitGraphError::MissingRoot);
    }

    #[test]
    fn root_commit_returns_single_root() {
        let mut g = CommitGraph::new();
        let root = root();
        let root_hash = root.hash();
        g.add_commit(root);

        assert_eq!(g.root_commit().expect("root").hash(), root_hash);
    }

    #[test]
    fn root_commit_rejects_multiple_roots() {
        let mut g = CommitGraph::new();
        g.add_commit(root_with_table(0));
        g.add_commit(root_with_table(1));

        assert_eq!(
            g.root_commit().unwrap_err(),
            CommitGraphError::MultipleRoots
        );
    }

    #[test]
    fn contains_known_and_unknown() {
        let mut g = CommitGraph::new();
        let first = c(1, vec![]);
        let first_hash = first.hash();
        g.add_commit(first);

        assert!(g.contains(&first_hash));
        assert!(!g.contains(&h(2)));
    }
}
