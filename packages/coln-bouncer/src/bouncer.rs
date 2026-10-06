//! The top level APIs exposed by the coln backend database to the user and FFI
use coln_flir_rs::{
    engine::{
        op::TabPathOp,
        packed::StoreTuple,
        schema::ColnDef,
        tx::{TxRowId, TxTuple},
    },
    hash::CommitHash,
    ir,
    public::{PublicRowId, PublicTuple},
    query::WhereClause,
};
use coln_query::api::{
    ColnQuery,
    deltas::{StoreDelta, TableDelta, ZRow},
    error::ColnQueryError,
    transaction::{TryCommitErr, TryCommitOk, Tx},
    violations::ViolationsDelta,
};
use coln_store::{
    store::{IdLookup, Store, auto::AutoStore},
    txn::rw::{StoreRead, StoreWrite},
};

use crate::{
    error::{BouncerError, RuleViolation},
    rw::{DbRead, DbWrite},
};

pub struct Bouncer {
    store: AutoStore,
    query: ColnQuery,
}

impl Bouncer {
    pub fn try_new(ir: ir::FlatRealm, coln_def: ColnDef) -> Result<Self, BouncerError> {
        let store = AutoStore::try_from_ir(&ir, coln_def)?;
        let query = ColnQuery::init(&ir)?;
        Ok(Bouncer { store, query })
    }

    /// # Panics
    ///
    /// If the initialisation fails
    pub fn new(ir: ir::FlatRealm, coln_def: ColnDef) -> Self {
        Self::try_new(ir, coln_def).expect("init success")
    }

    // fn apply_derived_view(
    //     &mut self,
    //     derived: DerivedDataDelta,
    //     commit: &Commit<'_>,
    // ) -> Result<(), StoreError> {
    //     let mut cnt = commit.num_ops as u32;
    //     let delta = derived.into_table_deltas();
    //     for td in delta {
    //         let oid = self.resolve_table(&td.for_entity().id().into()).ok_or(
    //             ValidationError::UnknownTable {
    //                 path: td.for_entity().id().into(),
    //             },
    //         )?;
    //         let table = self.tables.get_mut(&oid).expect("resolved correct table");

    //         let id_allocate = || {
    //             let row_id = PublicRowId {
    //                 commit: commit.hash(),
    //                 counter: cnt,
    //             };
    //             cnt += 1;
    //             self.store
    //                 .id_lookup()
    //                 .packed(&row_id)
    //                 .expect("hash already packed")
    //         };

    //         let ops = table.ops_from_table_delta(td, id_allocate);

    //         ops.into_iter().for_each(|op| table.stage_update(op));
    //         table.apply_staged_ops(&mut self.rowing)?;
    //     }
    //     Ok(())
    // }

    // // This conversion needs table schema, therefore cannot be done with From trait
    // pub(crate) fn ops_from_table_delta<F>(
    //     &self,
    //     td: TableDelta,
    //     mut id_allocate: F,
    // ) -> Vec<PackedOp>
    // where
    //     F: FnMut() -> PackedRowId,
    // {
    //     td.into_iter()
    //         .map(|zrow| {
    //             let row_id = id_allocate();
    //             if zrow.zweight() > 0 {
    //                 let mut val_iter = zrow.into_row().data.into_iter();
    //                 let mut packed_val = Vec::new();

    //                 for col in &self.schema().columns {
    //                     match col.col_type {
    //                         ir::ColType::RowId { .. } => {
    //                             let ScalarTypedValue::Uint(commit_idx) =
    //                                 val_iter.next().expect("coln-query returns valid data")
    //                             else {
    //                                 panic!("invalid data from coln-query");
    //                             };
    //                             let ScalarTypedValue::Uint(counter) =
    //                                 val_iter.next().expect("coln-query returns valid data")
    //                             else {
    //                                 panic!("invalid data from coln-query");
    //                             };
    //                             packed_val.push(PackedValue::Id(PackedRowId {
    //                                 commit_idx: commit_idx as u32,
    //                                 counter: counter as u32,
    //                             }));
    //                         }
    //                         ir::ColType::BuiltinTy {
    //                             builtin_ty: ir::BuiltinTy::BuiltinInt,
    //                         } => {
    //                             let ScalarTypedValue::String(s) = val_iter.next().unwrap() else {
    //                                 panic!("invalid data from coln-query");
    //                             };
    //                             packed_val.push(PackedValue::Str(s));
    //                         }
    //                         ir::ColType::BuiltinTy {
    //                             builtin_ty: ir::BuiltinTy::BuiltinStr,
    //                         } => {
    //                             let ScalarTypedValue::Iint(i) = val_iter.next().unwrap() else {
    //                                 panic!("invalid data from coln-query");
    //                             };
    //                             packed_val.push(PackedValue::Int(i as i32));
    //                         }
    //                     }
    //                 }

    //                 PackedOp::Add {
    //                     row_id,
    //                     values: packed_val.into(),
    //                 }
    //             } else if zrow.zweight() < 0 {
    //                 // TODO don't know how to remove yet
    //                 todo!()
    //             } else {
    //                 unreachable!("zero zweight impossible")
    //             }
    //         })
    //         .collect()
    // }

    // fn table_delta_from_ops(&self, ops: impl IntoIterator<Item = PackedOp>) -> TableDelta {
    //     let zrows: Vec<ZRow> = ops
    //         .into_iter()
    //         .map(|op| match op {
    //             PackedOp::Add { row_id, values } => {
    //                 let tuple = PackedRowView { row_id, values }.into();
    //                 ZRow::new(1, tuple).unwrap()
    //             }
    //             PackedOp::Delete { row_id } => {
    //                 let values = self
    //                     .row_by_id(row_id)
    //                     .expect("element to delete should exist");
    //                 let tuple = PackedRowView { row_id, values }.into();
    //                 ZRow::new(-1, tuple).unwrap()
    //             }
    //         })
    //         .collect();

    //     TableDelta::new(self.path().to_string(), zrows)
    // }

    // fn check_rules(&mut self) -> Result<DerivedDataDelta, StoreError> {
    //     match query_tx.try_commit(&mut self.cq) {
    //         Ok(TryCommitOk::Pending(pending)) => {
    //             let mut committed = pending.commit()?;
    //             let derived = committed.take_derived_data_delta();
    //             let monitored_constraints = committed.take_soft_violations();
    //             if !monitored_constraints.is_empty() {
    //                 tracing::warn!("monitored constraint violation {}", monitored_constraints);
    //             }
    //             Ok(derived)
    //         }
    //         Ok(TryCommitOk::Rejected(mut rejected)) => {
    //             let violations = rejected.take_hard_violations();
    //             Err(error::RuleViolation::HardViolation(violations).into())
    //         }
    //         Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {
    //             Err(err.into())
    //         }
    //     }
    // }
}

impl coln_store::store::frag::FragmentSync for Bouncer {
    fn commit_chunks_after(
        &self,
        have_heads: &[CommitHash],
    ) -> Vec<coln_store::store::frag::CommitChunk> {
        self.store.commit_chunks_after(have_heads)
    }

    fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), coln_store::store::error::StoreError> {
        // TODO need to change this workflow to something like
        // decode the bytes into commit
        // Apply what can be applied and return the tuples that can be applied
        // while making the store hold in a pending state
        // feed those into coln-query for rule checking
        // If ok, ask the store to apply the changes.
        self.store.apply_chunk_bytes(chunk_bytes)
    }
}

impl Bouncer {
    /// Helper to unpack a tuple from store.
    /// This method lives here rather than coln-flir-rs because it is requires
    /// an API from coln-store
    fn unpack_store_tuple(&self, st: StoreTuple) -> PublicTuple {
        let id_lookup = self.store.id_lookup();
        PublicTuple::from_store(st, |packed| {
            id_lookup
                .unpacked(&packed)
                .expect("tuples from store can be unpacked")
        })
    }
}

impl DbRead for Bouncer {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<PublicTuple>> {
        Some(
            self.store
                .scan_table(table)?
                .into_iter()
                .map(|st| self.unpack_store_tuple(st))
                .collect(),
        )
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &PublicRowId) -> Option<PublicTuple> {
        // An unknown commit hash means the row cannot exist in this store.
        let packed = self.store.id_lookup().packed(row_id)?;
        self.store
            .row_by_id(table, &packed)
            .map(|st| self.unpack_store_tuple(st))
    }

    fn all_proj(
        &self,
        query: &WhereClause,
        select: &[u32],
    ) -> Result<Vec<PublicTuple>, BouncerError> {
        Ok(self
            .store
            .all_proj(query, select)?
            .into_iter()
            .map(|st| self.unpack_store_tuple(st))
            .collect())
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<PublicRowId>, BouncerError> {
        let id_lookup = self.store.id_lookup();
        Ok(self
            .store
            .all_row_id(query)?
            .into_iter()
            .map(|packed| {
                id_lookup
                    .unpacked(&packed)
                    .expect("row ids from store can be unpacked")
            })
            .collect())
    }
}

impl DbWrite for Bouncer {
    fn transaction(&mut self) {
        self.store.transaction();
    }

    fn commit(&mut self) -> Result<ViolationsDelta, BouncerError> {
        let tuples = self.store.try_commit()?;
        let mut tx = Tx::new(StoreDelta::empty());
        // TODO convert TabPathOp into TableDelta
        tx.insert(std::iter::once(TableDelta::new("SomeTable", [])));

        match tx.try_commit(&mut self.query) {
            Ok(TryCommitOk::Pending(pending)) => {
                let mut committed = pending.commit()?;
                let derived_data_delta = committed.take_derived_data_delta();
                let monitored_constraints = committed.take_soft_violations();
                // TODO convert TableDelta back into TabPathOp
                // TODO and apply them
                Ok(monitored_constraints)
            }
            Ok(TryCommitOk::Rejected(mut rejected)) => {
                // A hard constraint has been violated.
                // At this point in time coln-query has already rolled back
                // its internal state.
                // In this arm, coln-store must roll back to keep the two in
                // sync and report back the violations:
                let violations = rejected.take_hard_violations();
                Err(RuleViolation::HardViolation(violations).into())
            }
            // The next two can also be combined, as they share the same error
            // type and should be considered a bug, I guess.
            // Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {}
            Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {
                Err(BouncerError::QueryError(err))
            }
        }
    }

    fn abort(&mut self) {
        self.store.abort();
    }

    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, BouncerError> {
        Ok(self.store.add(table, values)?)
    }
}
