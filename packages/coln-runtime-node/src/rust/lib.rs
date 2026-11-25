// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Node bindings for coln-bouncer.
//!
//! The bouncer's write transaction is a typestate machine that consumes itself
//! on each transition, which JS cannot express. Instead, writes are scoped to a
//! callback:
//! [`Bouncer::begin_write`] opens a transaction, hands JS a [`WriteTxn`] handle
//! for the duration of the callback, and commits when the callback returns. To
//! roll back, the callback throws. Reads outside a write open an implicit read
//! transaction per call.
//!
//! Values cross the boundary as JSON strings, decoded by the TypeScript wrapper.

use std::{cell::RefCell, rc::Rc};

use coln_bouncer::bouncer::{
    self, ReadTxnable,
    txn::{Prepare, WriteTxn as BouncerWriteTxn},
};
use coln_bouncer::error::BouncerError;
use coln_flir_rs::{
    engine::{schema::ColnDef, tx::TxRowId},
    hash::CommitHash,
    ir,
    public::PublicRowId,
    query::WhereClause,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::{Serialize, de::DeserializeOwned};

fn napi_error(msg: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, msg.to_string())
}

fn bouncer_error(e: BouncerError) -> Error {
    napi_error(e)
}

fn from_json<T: DeserializeOwned>(what: &str, json: &str) -> Result<T> {
    serde_json::from_str(json).map_err(|e| napi_error(format!("invalid {what}: {e}")))
}

fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(napi_error)
}

/// The JSON-in/JSON-out read API shared by implicit reads and write transactions.
fn read_all_proj(r: &impl ReadTxnable, query: &str, select: &str) -> Result<String> {
    let query: WhereClause = from_json("where clause", query)?;
    let select: Vec<u32> = from_json("select", select)?;
    to_json(&r.all_proj(&query, &select).map_err(bouncer_error)?)
}

fn read_all_row_id(r: &impl ReadTxnable, query: &str) -> Result<String> {
    let query: WhereClause = from_json("where clause", query)?;
    to_json(&r.all_row_id(&query).map_err(bouncer_error)?)
}

fn read_one_proj(r: &impl ReadTxnable, query: &str, select: &str) -> Result<String> {
    let query: WhereClause = from_json("where clause", query)?;
    let select: Vec<u32> = from_json("select", select)?;
    to_json(&r.one_proj(&query, &select).map_err(bouncer_error)?)
}

fn read_exists(r: &impl ReadTxnable, query: &str) -> Result<bool> {
    let query: WhereClause = from_json("where clause", query)?;
    r.exists(&query).map_err(bouncer_error)
}

fn read_scan_table(r: &impl ReadTxnable, table: String) -> Result<Option<String>> {
    r.scan_table(&ir::Path(table))
        .map(|rows| to_json(&rows))
        .transpose()
}

#[napi]
pub struct Bouncer {
    // Rust borrow checker ensures that no two references can be borrowed if one
    // of them is a mut ref. But JS side has no knowledge of that. So we turn this
    // into a runtime check. Someone has to panic somewhere, probably should be here
    // than panicing in the core rust code.
    inner: RefCell<bouncer::Bouncer>,
}

#[napi]
pub fn bouncer_from_ir(
    ir_json: String,
    coln_source: String,
    realm_name: String,
) -> Result<Bouncer> {
    let ir = from_json("flir", &ir_json)?;
    let coln_def = ColnDef::new(coln_source, realm_name);
    let bouncer = bouncer::Bouncer::try_new(ir, coln_def).map_err(bouncer_error)?;
    Ok(Bouncer {
        inner: RefCell::new(bouncer),
    })
}

/// Returned by [`Bouncer::begin_write`] as JSON.
#[derive(Serialize)]
struct CommitResult {
    hash: CommitHash,
    /// The canonical public id of every row added in this transaction, in the
    /// order `add` was called.
    row_ids: Vec<PublicRowId>,
}

impl Bouncer {
    fn read(&self) -> Result<bouncer::ReadTxn> {
        let bouncer = self.inner.try_borrow().map_err(|_| {
            napi_error("cannot read from the bouncer inside a write transaction; read through the transaction instead")
        })?;
        Ok(bouncer.begin_read())
    }
}

#[napi]
impl Bouncer {
    /// Run `callback` against a fresh write transaction, then commit it.
    ///
    /// Returns the commit result as JSON. If the callback throws, the
    /// transaction is discarded and the error is rethrown. If committing fails
    /// (e.g. a hard rule violation), nothing is applied and an error is thrown.
    #[napi(ts_args_type = "callback: (txn: WriteTxn) => void")]
    pub fn begin_write(
        &self,
        env: Env,
        callback: Function<ClassInstance<WriteTxn>, Unknown>,
    ) -> Result<String> {
        // Held for the whole call as a reentrancy guard: a nested write or
        // import would otherwise block on the store lock this transaction holds.
        let bouncer = self
            .inner
            .try_borrow_mut()
            .map_err(|_| napi_error("a write transaction is already in progress"))?;
        // The transaction holds the store's upgradable read lock until it is
        // committed or dropped. `_close` takes it back out of the shared slot
        // when this call returns (also on unwind), so a copy of the JS handle
        // that escapes the callback only sees `Slot::Closed` and cannot keep the
        // lock alive and deadlock later writes.
        let txn = bouncer.begin_write();
        let slot = Rc::new(RefCell::new(Slot::Open {
            txn,
            added: Vec::new(),
        }));
        let _close = CloseOnDrop(Rc::clone(&slot));

        let handle = WriteTxn {
            slot: Rc::clone(&slot),
        }
        .into_instance(&env)?;
        let outcome = callback.call(handle);
        let state = slot.replace(Slot::Closed);

        let (txn, added) = match (outcome, state) {
            // Dropping a `Prepare` txn discards its queued operations.
            (Err(e), _) => return Err(e),
            (Ok(_), Slot::Closed) => unreachable!("only begin_write closes the slot"),
            (Ok(_), Slot::Open { txn, added }) => (txn, added),
        };
        let committed = txn.commit().map_err(bouncer_error)?;
        let hash = committed.cmt_hash();
        let row_ids = bouncer.begin_read().promote(added, hash);
        to_json(&CommitResult { hash, row_ids })
    }

    // Read methods each open an implicit read transaction. Lock guards must
    // not outlive a call: the store's locks are blocking, and JS is single
    // threaded, so a read guard held across calls would deadlock the next
    // commit.

    #[napi(catch_unwind)]
    pub fn all_proj(&self, query: String, select: String) -> Result<String> {
        read_all_proj(&self.read()?, &query, &select)
    }

    #[napi(catch_unwind)]
    pub fn all_row_id(&self, query: String) -> Result<String> {
        read_all_row_id(&self.read()?, &query)
    }

    #[napi(catch_unwind)]
    pub fn one_proj(&self, query: String, select: String) -> Result<String> {
        read_one_proj(&self.read()?, &query, &select)
    }

    #[napi(catch_unwind)]
    pub fn exists(&self, query: String) -> Result<bool> {
        read_exists(&self.read()?, &query)
    }

    /// All rows of `table` as JSON, or `null` if there is no such table.
    #[napi]
    pub fn scan_table(&self, table: String) -> Result<Option<String>> {
        read_scan_table(&self.read()?, table)
    }

    /// The current heads, as a JSON array of hex commit hashes.
    #[napi]
    pub fn heads(&self) -> Result<String> {
        to_json(&self.read()?.heads())
    }

    /// Every commit not reachable from `have_heads` (a JSON array of hex
    /// commit hashes), in dependency order, as an array of `Fragment`s.
    #[napi(ts_return_type = "import('@coln-project/interface').Fragment[]")]
    pub fn fragments_after<'env>(
        &self,
        env: &'env Env,
        have_heads: String,
    ) -> Result<Unknown<'env>> {
        let have_heads: Vec<CommitHash> = from_json("heads", &have_heads)?;
        env.to_js_value(&self.read()?.fragments_after(&have_heads))
    }

    /// Decode and apply commits received from another replica.
    ///
    /// Each ready commit is validated and applied on its own: if one is
    /// rejected, the commits applied before it stay applied and an error is
    /// thrown. Commits whose parents are missing are kept pending.
    #[napi]
    pub fn apply_chunk_bytes(&self, chunks: Vec<Uint8Array>) -> Result<()> {
        let bouncer = self
            .inner
            .try_borrow_mut()
            .map_err(|_| napi_error("cannot apply commits inside a write transaction"))?;
        // TODO surface the soft violations of each applied commit once
        // `ViolationsDelta` is serializable.
        bouncer
            .begin_import()
            .apply_chunk_bytes(chunks.iter().map(|c| c.to_vec()))
            .map_err(bouncer_error)?;
        Ok(())
    }
}

enum Slot {
    Open {
        txn: BouncerWriteTxn<Prepare>,
        added: Vec<TxRowId>,
    },
    Closed,
}

struct CloseOnDrop(Rc<RefCell<Slot>>);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        // Only fails if a `WriteTxn` method is mid-call, which cannot happen
        // while `begin_write` is unwinding on the same thread.
        if let Ok(mut slot) = self.0.try_borrow_mut() {
            *slot = Slot::Closed;
        }
    }
}

/// A write transaction, valid only inside the `Bouncer.beginWrite` callback.
///
/// Reads see committed data only, not rows added in this transaction.
#[napi]
pub struct WriteTxn {
    slot: Rc<RefCell<Slot>>,
}

impl WriteTxn {
    fn with_open<T>(
        &self,
        f: impl FnOnce(&mut BouncerWriteTxn<Prepare>, &mut Vec<TxRowId>) -> Result<T>,
    ) -> Result<T> {
        match &mut *self.slot.borrow_mut() {
            Slot::Open { txn, added } => f(txn, added),
            Slot::Closed => Err(napi_error(
                "this transaction has ended; it can only be used inside its beginWrite callback",
            )),
        }
    }
}

#[napi]
impl WriteTxn {
    /// Queue a row; returns its pending row id as JSON.
    #[napi]
    pub fn add(&self, table_name: String, values: String) -> Result<String> {
        let values: coln_flir_rs::engine::tx::TxTuple = from_json("tuple", &values)?;
        self.with_open(|txn, added| {
            let id = txn
                .add(&ir::Path(table_name), values)
                .map_err(bouncer_error)?;
            added.push(id);
            to_json(&id)
        })
    }

    #[napi(catch_unwind)]
    pub fn all_proj(&self, query: String, select: String) -> Result<String> {
        self.with_open(|txn, _| read_all_proj(txn, &query, &select))
    }

    #[napi(catch_unwind)]
    pub fn all_row_id(&self, query: String) -> Result<String> {
        self.with_open(|txn, _| read_all_row_id(txn, &query))
    }

    #[napi(catch_unwind)]
    pub fn one_proj(&self, query: String, select: String) -> Result<String> {
        self.with_open(|txn, _| read_one_proj(txn, &query, &select))
    }

    #[napi(catch_unwind)]
    pub fn exists(&self, query: String) -> Result<bool> {
        self.with_open(|txn, _| read_exists(txn, &query))
    }

    #[napi]
    pub fn scan_table(&self, table: String) -> Result<Option<String>> {
        self.with_open(|txn, _| read_scan_table(txn, table))
    }
}
