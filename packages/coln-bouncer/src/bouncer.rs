//! The top level APIs exposed by the coln backend database to the user and FFI
use coln_flir_rs::{
    WireRowId, WireRowView, WireTuple,
    engine::{
        schema::ColnDef,
        txn_val::{TxnWireRowId, TxnWireTuple},
    },
    hash::CommitHash,
    ir,
    query::WhereClause,
};

use crate::{
    error::BouncerError,
    rw::{DbRead, DbWrite},
};

pub struct Bouncer {
    store: coln_store::store::Store,
    query: coln_query::api::ColnQuery,
}

impl Bouncer {
    pub fn new(ir: ir::FlatRealm, cd: ColnDef) -> Self {
        todo!()
    }

    pub fn try_new(ir: ir::FlatRealm, cd: ColnDef) -> Result<Self, BouncerError> {
        todo!()
    }

    fn apply_derived_view(
        &mut self,
        derived: DerivedDataDelta,
        commit: &Commit<'_>,
    ) -> Result<(), StoreError> {
        let mut cnt = commit.num_ops as u32;
        let delta = derived.into_table_deltas();
        for td in delta {
            let oid = self.resolve_table(&td.for_entity().id().into()).ok_or(
                ValidationError::UnknownTable {
                    path: td.for_entity().id().into(),
                },
            )?;
            let table = self.tables.get_mut(&oid).expect("resolved correct table");

            let id_allocate = || {
                let row_id = WireRowId {
                    commit: commit.hash(),
                    counter: cnt,
                };
                cnt += 1;
                self.id_packer
                    .lookup_row_id(&row_id)
                    .expect("hash already packed")
            };

            let ops = table.ops_from_table_delta(td, id_allocate);

            ops.into_iter().for_each(|op| table.stage_update(op));
            table.apply_staged_ops(&mut self.rowing)?;
        }
        Ok(())
    }

    // This conversion needs table schema, therefore cannot be done with From trait
    pub(crate) fn ops_from_table_delta<F>(
        &self,
        td: TableDelta,
        mut id_allocate: F,
    ) -> Vec<PackedOp>
    where
        F: FnMut() -> PackedRowId,
    {
        td.into_iter()
            .map(|zrow| {
                let row_id = id_allocate();
                if zrow.zweight() > 0 {
                    let mut val_iter = zrow.into_row().data.into_iter();
                    let mut packed_val = Vec::new();

                    for col in &self.schema().columns {
                        match col.col_type {
                            ir::ColType::RowId { .. } => {
                                let ScalarTypedValue::Uint(commit_idx) =
                                    val_iter.next().expect("coln-query returns valid data")
                                else {
                                    panic!("invalid data from coln-query");
                                };
                                let ScalarTypedValue::Uint(counter) =
                                    val_iter.next().expect("coln-query returns valid data")
                                else {
                                    panic!("invalid data from coln-query");
                                };
                                packed_val.push(PackedValue::Id(PackedRowId {
                                    commit_idx: commit_idx as u32,
                                    counter: counter as u32,
                                }));
                            }
                            ir::ColType::BuiltinTy {
                                builtin_ty: ir::BuiltinTy::BuiltinInt,
                            } => {
                                let ScalarTypedValue::String(s) = val_iter.next().unwrap() else {
                                    panic!("invalid data from coln-query");
                                };
                                packed_val.push(PackedValue::Str(s));
                            }
                            ir::ColType::BuiltinTy {
                                builtin_ty: ir::BuiltinTy::BuiltinStr,
                            } => {
                                let ScalarTypedValue::Iint(i) = val_iter.next().unwrap() else {
                                    panic!("invalid data from coln-query");
                                };
                                packed_val.push(PackedValue::Int(i as i32));
                            }
                        }
                    }

                    PackedOp::Add {
                        row_id,
                        values: packed_val.into(),
                    }
                } else if zrow.zweight() < 0 {
                    // TODO don't know how to remove yet
                    todo!()
                } else {
                    unreachable!("zero zweight impossible")
                }
            })
            .collect()
    }

    fn table_delta_from_ops(&self, ops: impl IntoIterator<Item = PackedOp>) -> TableDelta {
        let zrows: Vec<ZRow> = ops
            .into_iter()
            .map(|op| match op {
                PackedOp::Add { row_id, values } => {
                    let tuple = PackedRowView { row_id, values }.into();
                    ZRow::new(1, tuple).unwrap()
                }
                PackedOp::Delete { row_id } => {
                    let values = self
                        .row_by_id(row_id)
                        .expect("element to delete should exist");
                    let tuple = PackedRowView { row_id, values }.into();
                    ZRow::new(-1, tuple).unwrap()
                }
            })
            .collect();

        TableDelta::new(self.path().to_string(), zrows)
    }

    fn check_rules(&mut self) -> Result<DerivedDataDelta, StoreError> {
        match query_tx.try_commit(&mut self.cq) {
            Ok(TryCommitOk::Pending(pending)) => {
                let mut committed = pending.commit()?;
                let derived = committed.take_derived_data_delta();
                let monitored_constraints = committed.take_soft_violations();
                if !monitored_constraints.is_empty() {
                    tracing::warn!("monitored constraint violation {}", monitored_constraints);
                }
                Ok(derived)
            }
            Ok(TryCommitOk::Rejected(mut rejected)) => {
                let violations = rejected.take_hard_violations();
                Err(error::RuleViolation::HardViolation(violations).into())
            }
            Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {
                Err(err.into())
            }
        }
    }
}

impl coln_store::store::frag::FragmentSync for Bouncer {
    fn commit_chunks_after(
        &self,
        have_heads: &[CommitHash],
    ) -> Vec<coln_store::store::frag::CommitChunk> {
        todo!()
    }

    fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), coln_store::store::error::StoreError> {
        todo!()
    }
}

impl DbRead for Bouncer {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>> {
        todo!()
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &WireRowId) -> Option<WireRowView> {
        todo!()
    }

    fn all_proj(
        &self,
        query: &WhereClause,
        select: &[u32],
    ) -> Result<Vec<WireTuple>, BouncerError> {
        todo!()
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<WireRowId>, BouncerError> {
        todo!()
    }
}

impl DbWrite for Bouncer {
    fn transaction(&mut self) {
        todo!()
    }

    fn commit(&mut self) {
        todo!()
    }

    fn abort(&mut self) {
        todo!()
    }

    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TxnWireRowId, BouncerError> {
        todo!()
    }
}
