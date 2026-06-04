use coln_lang_rs::ir;
use tracing::info;

use crate::{
    commit::{Commit, author::Author, hash::CommitHash, wire::CommitData},
    store::{Store, error::StoreIntError},
    table::ValidationError,
    txn::{
        ops::{PendingOp, TempRowId, TxnCellValue},
        timestamp::Timestamp,
    },
};

pub(crate) struct TxnInner {
    deps: Vec<CommitHash>,
    author: Author,
    pending: Vec<PendingOp>,
    timestamp: Timestamp,
    message: Option<String>,
}

impl TxnInner {
    pub(crate) fn new(deps: Vec<CommitHash>) -> Self {
        Self {
            deps,
            author: Author::foo(),
            pending: Vec::new(),
            timestamp: Timestamp::now(),
            message: None,
        }
    }

    fn next_id(&self) -> TempRowId {
        TempRowId(self.pending.len() as u32)
    }

    pub(crate) fn add(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, Box<StoreIntError>> {
        let t = store.table_at(table).ok_or(ValidationError::UnknownTable {
            path: table.clone(),
        })?;
        t.validate_column_count(values.len())?;
        let id = self.next_id();
        self.pending.push(PendingOp::Add {
            row_id: id,
            table: table.clone(),
            values,
        });
        Ok(id)
    }

    pub(crate) fn commit(self, store: &mut Store) -> Result<CommitHash, Box<StoreIntError>> {
        info!(op_count = self.pending.len(), "commit txn");
        let cmt = Commit::from_commit_data(
            CommitData::new(
                self.deps,
                self.author,
                *self.timestamp.as_ref(),
                self.message,
                self.pending,
            ),
            |path| store.table_at(path).map(|table| table.schema()),
        )?;
        let h = cmt.hash();
        store.apply_commit(cmt)?;
        info!("applied batch");
        Ok(h)
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
