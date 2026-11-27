// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The public database interface, with explicit read and write transactions.
//!
//! Use [`Bouncer::begin_read`] for public tuples, ids, and commit history, and
//! [`Bouncer::begin_write`] to stage local operations. Committing consumes the
//! write transaction and validates rules before accepting base and derived data.
//! Dropping or aborting it discards its operations.
//! Immutable catalog metadata is available directly through [`Bouncer::coln_def`]
//! and [`Bouncer::json_ir`], without opening a transaction.
//!
//! [`Bouncer::begin_import`] creates a separate write transaction for incoming
//! commits. Imports accept ready commits individually, so an error does not undo
//! earlier accepted commits. Filtering and projection helpers remain unimplemented.

mod persist;
pub mod txn;

use coln_flir_rs::{engine::schema::ColnDef, ir};
use coln_query::api::{ColnQuery, violations::ViolationsDelta};
use coln_store::store::Store;

use crate::error::BouncerError;

pub use txn::{ImportTxn, ReadTxn, ReadTxnable, WriteTxn};

pub struct Bouncer {
    store: Store,
    query: ColnQuery,
}

impl Bouncer {
    /// Creates a new [`Bouncer`] from the IR `ir` and coln source `coln_def`.
    pub fn try_new(ir: ir::FlatRealm, coln_def: ColnDef) -> Result<Self, BouncerError> {
        let store = Store::try_from_ir(&ir, coln_def)?;
        let query = ColnQuery::init(&ir)?;
        Ok(Bouncer { store, query })
    }

    /// # Panics
    ///
    /// If the initialisation fails
    pub fn new(ir: ir::FlatRealm, coln_def: ColnDef) -> Self {
        Self::try_new(ir, coln_def).expect("init success")
    }

    fn new_from_store_query(store: Store, query: ColnQuery) -> Self {
        Bouncer { store, query }
    }

    pub fn coln_def(&self) -> &ColnDef {
        self.store.coln_def()
    }

    pub fn json_ir(&self) -> String {
        self.store.json_ir()
    }

    /// Attempt to build a bouncer from commit chunks from scratch, assuming that
    /// the input `chunk_bytes` contains a valid root commit.
    pub fn try_from_commit_bytes(
        bytes: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<(Self, Vec<ViolationsDelta>), BouncerError> {
        let (store, commits) = Store::try_from_commits_bytes(bytes)?;
        // nothing to drain here
        let query = ColnQuery::init(store.ir())?;
        let bouncer = Bouncer::new_from_store_query(store, query);

        let batch = bouncer.store.prepare_commits(commits, false)?;
        let violations = bouncer.begin_import().apply_batch(batch)?;
        Ok((bouncer, violations))
    }
}
