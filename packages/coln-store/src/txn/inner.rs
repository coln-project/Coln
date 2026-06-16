use coln_lang_rs::ir;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

use crate::{
    commit::{Commit, author::Author, hash::CommitHash, wire::CommitData},
    store::{Store, error::StoreIntError},
    table::ValidationError,
    txn::{
        ops::{PendingOp, RowHandle, TempRowId, TxnCellValue, TxnId, TxnValue},
        timestamp::Timestamp,
    },
};

static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);

fn next_tx_id() -> TxnId {
    TxnId::new(NEXT_TX_ID.fetch_add(1, Ordering::Relaxed))
}

pub(crate) struct TxnInner {
    deps: Vec<CommitHash>,
    author: Author,
    pending: Vec<PendingOp>,
    timestamp: Timestamp,
    message: Option<String>,
    tx_id: TxnId,
    pending_handles: Vec<RowHandle>,
}

impl TxnInner {
    pub(crate) fn new(deps: Vec<CommitHash>) -> Self {
        Self {
            deps,
            author: Author::foo(),
            pending: Vec::new(),
            timestamp: Timestamp::now(),
            message: None,
            tx_id: next_tx_id(),
            pending_handles: Vec::new(),
        }
    }

    fn next_id(&self) -> TempRowId {
        TempRowId::from(self.pending.len() as u32)
    }

    fn add_cell_values(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, Box<StoreIntError>> {
        let t = store.table_at(table).ok_or(ValidationError::UnknownTable {
            path: table.clone(),
        })?;
        t.validate_column_count(values.len())?;
        let temp_id = self.next_id();
        self.pending.push(PendingOp::Add {
            row_id: temp_id,
            table: table.clone(),
            values,
        });
        Ok(temp_id)
    }

    pub(crate) fn add(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: Vec<TxnValue>,
    ) -> Result<RowHandle, Box<StoreIntError>> {
        let txn_values = values
            .into_iter()
            .map(|v| v.to_txn_cell_value(self.tx_id))
            .collect::<Result<Vec<TxnCellValue>, _>>()?;
        let temp_id = self.add_cell_values(store, table, txn_values)?;
        let handle = RowHandle::pending(self.tx_id, temp_id);
        self.pending_handles.push(handle.clone());
        Ok(handle)
    }

    // Used by the REPL only
    #[cfg(feature = "native")]
    pub(crate) fn add_internal(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, Box<StoreIntError>> {
        self.add_cell_values(store, table, values)
    }

    fn invalidate_handles(pending_handles: Vec<RowHandle>, reason: &str) {
        pending_handles
            .into_iter()
            .for_each(|h| h.invalidate(reason));
    }

    fn finalize_handles(pending_handles: Vec<RowHandle>, h: CommitHash) {
        pending_handles
            .into_iter()
            .for_each(|handle| handle.finalize(h));
    }

    pub(crate) fn commit(self, store: &mut Store) -> Result<CommitHash, Box<StoreIntError>> {
        info!(op_count = self.pending.len(), "commit txn");
        let TxnInner {
            deps,
            author,
            pending,
            timestamp,
            message,
            pending_handles,
            ..
        } = self;
        let cmt = Commit::from_commit_data(
            CommitData::new(deps, author, *timestamp.as_ref(), message, pending),
            |path| store.table_at(path).map(|table| table.schema()),
        );
        let cmt = match cmt {
            Ok(cmt) => cmt,
            Err(err) => {
                Self::invalidate_handles(pending_handles, "txn commit encoding failed");
                return Err(err.into());
            }
        };

        let h = cmt.hash();
        match store.apply_commit(cmt) {
            Ok(()) => {
                Self::finalize_handles(pending_handles, h);
                info!("applied batch");
                Ok(h)
            }
            Err(err) => {
                Self::invalidate_handles(pending_handles, "txn commit failed");
                Err(err)
            }
        }
        // 1. validate full batch (PK conflicts including intra-batch)
        // 2. compute hash: blake3(deps || timestamp || message || canonical(ops))
        // 3. resolve: TxnRowId(k) -> RowId { commit: hash, counter: k }
        //             CellValue::TxnId(k) -> CellValue::Id(RowId { commit: hash, counter: k })
        // 4. apply resolved Ops to tables via table.append_row
        // 5. check_laws
        // 6. push CommitMeta into store.commit_graph, advance heads
        // 7. return hash
    }
}
