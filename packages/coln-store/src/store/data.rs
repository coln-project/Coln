// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::{HashMap, HashSet};

use coln_flir_rs::public::PublicScalarValue;
use tracing::info;

use crate::{
    commit::{DataCommit, graph::CommitGraph},
    op::PublicOp,
    pack::{IdPacker, IdPackerSnapshot},
    rollback::Rollback,
    rowing::{self, RowingSnapshot},
    store::{
        Catalog,
        error::{CommitApplyError, StoreError},
    },
    table::{Table, TableOid, TableSnapshot, ValidationError},
};

#[derive(Debug)]
pub(super) struct StoreData {
    /// Oids are dense and tables are never dropped, so the next oid is `tables.len()`.
    pub(super) tables: HashMap<TableOid, Table>,
    pub(super) id_packer: IdPacker,
    pub(super) commits: CommitGraph,
    pub(super) rowing: rowing::Rowing,
    // buffered commits not ready to be applied to the store due to missing deps
    pub(super) pending_commits: Vec<DataCommit>,
}

pub(crate) struct DataSnapshot {
    tables: Vec<(TableOid, TableSnapshot)>,
    id_packer: IdPackerSnapshot,
    rowing: RowingSnapshot,
}

impl Rollback for StoreData {
    type Snapshot = DataSnapshot;

    fn snapshot(&mut self) -> Self::Snapshot {
        let tables = self
            .tables
            .iter_mut()
            .map(|(&oid, table)| (oid, table.snapshot()))
            .collect();
        let id_packer = self.id_packer.snapshot();
        let rowing = self.rowing.snapshot();
        DataSnapshot {
            tables,
            id_packer,
            rowing,
        }
    }

    fn commit(&mut self, snapshot: Self::Snapshot) {
        let DataSnapshot {
            tables,
            id_packer,
            rowing,
        } = snapshot;
        for (oid, snapshot) in tables {
            self.tables
                .get_mut(&oid)
                .expect("snapshotted table should still exist")
                .commit(snapshot);
        }
        self.id_packer.commit(id_packer);
        self.rowing.commit(rowing);
    }

    fn rollback_to(&mut self, snapshot: Self::Snapshot) {
        let DataSnapshot {
            tables,
            id_packer,
            rowing,
        } = snapshot;
        for (oid, snapshot) in tables {
            self.tables
                .get_mut(&oid)
                .expect("snapshotted table should still exist")
                .rollback_to(snapshot);
        }
        self.id_packer.rollback_to(id_packer);
        self.rowing.rollback_to(rowing);
    }
}

impl StoreData {
    /// Add commit to the commit graph. This is a low level API, typically you
    /// want to use `apply_commits`
    pub(crate) fn record_in_commit_graph(&mut self, commit: DataCommit) {
        self.commits.add_commit(commit);
    }


}

impl StoreData {
    /// Applying the data, assuming that it has passed the format checker, i.e.
    /// the data conforms the the schema type definitions.
    /// But it might not follow all the rule definitions, it might also violate
    /// primary key constraints after hashconsing
    pub(super) fn apply_commit_ops(&mut self, ops: Vec<PublicOp>) -> Result<(), StoreError> {
        let op_count = ops.len();
        let affected = self.stage_commit_ops(ops);
        self.apply_staged_ops(&affected)?;

        info!(op_count, "applied batch");
        Ok(())
    }

    // Stage all the commit ops into the table's pending state.
    fn stage_commit_ops(&mut self, ops: Vec<PublicOp>) -> Vec<TableOid> {
        let mut affected = HashSet::new();
        for op in ops {
            let oid = op.table();
            let op = self.id_packer.pack_row(op);
            self.tables
                .get_mut(&oid)
                .expect("validated batch")
                .stage_update(op);
            affected.insert(oid);
        }
        affected.into_iter().collect()
    }

    pub(super) fn apply_staged_ops(&mut self, tables: &[TableOid]) -> Result<(), StoreError> {
        for oid in tables {
            self.tables
                .get_mut(oid)
                .expect("staged table exists")
                .apply_staged_ops(&mut self.rowing)?;
        }
        self.rowing.apply_unions(&self.id_packer);
        Ok(())
    }

    /// Make sure that the decoded commit has sane deps and is not already applied
    pub(super) fn check_commit_deps<'a>(
        &self,
        commits: impl Iterator<Item = &'a DataCommit>,
    ) -> Result<(), StoreError> {
        let mut seen_hashes = HashSet::new();
        for commit in commits {
            let hash = commit.hash();
            if self.commits.contains(&hash) {
                continue;
            }

            if commit.deps.is_empty() {
                return Err(CommitApplyError::DanglingCommit(hash).into());
            }

            if seen_hashes.contains(&hash) {
                return Err(CommitApplyError::DuplicateCommit(hash).into());
            }

            seen_hashes.insert(hash);
        }

        Ok(())
    }

    // We do as much check as possible without making changes to the tables
    // including checks like:
    //  - data following schema format
    //  - no duplication of primary keys before hashconsing
    pub(super) fn check_commit_ops(
        &self,
        cmt: DataCommit,
        catalog: &Catalog,
    ) -> Result<CheckedOpCommit, StoreError> {
        // TODO perhaps use late resolution, i.e. not resolving any ids, and when
        // we resolve, immediately make them packed.
        let ops = cmt.resolved_ops(|path| {
            catalog
                .resolve_table(path)
                .and_then(|oid| catalog.table_meta(oid))
        })?;

        // TODO also need to validate that ids in op is referring to an existing id
        let mut pending_pk: HashMap<TableOid, Vec<Vec<PublicScalarValue>>> = HashMap::new();
        for op in &ops {
            let (table_oid, values) = match op {
                PublicOp::Add { table, values } => (table, values),
                PublicOp::Delete { .. } => todo!("validate delete operations"),
            };
            let t = self
                .tables
                .get(table_oid)
                .ok_or(ValidationError::UnknownTableOid { oid: *table_oid })?;
            t.validate_insert(&values[1..], &self.id_packer)?;

            // Check primary key conflicts within ops batch
            if let Some(key) = t.primary_key_values(&values[1..]) {
                let keys = pending_pk.entry(*table_oid).or_default();
                if keys.iter().any(|k| k == &key) {
                    return Err(ValidationError::DuplicatePrimaryKey.into());
                }
                keys.push(key);
            }
        }

        Ok(CheckedOpCommit { ops, original: cmt })
    }


    pub fn has_pending(&self) -> bool {
        !self.pending_commits.is_empty()
    }

}

pub(super) struct CheckedOpCommit {
    pub(super) ops: Vec<PublicOp>,
    pub(super) original: DataCommit,
}
