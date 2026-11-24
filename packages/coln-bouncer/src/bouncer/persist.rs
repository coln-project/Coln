// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_query::api::{ColnQuery, violations::ViolationsDelta};
use coln_store::store::{Store, error::StoreError, txn::ReadTxnable};

use crate::{
    bouncer::Bouncer,
    error::BouncerError::{self},
};

impl Bouncer {
    /// Given bytes stored on disk, decode them and create a bouncer out of it
    /// Returns a `Vec<ViolationsDelta>` indicating the cumulative monitored
    /// contraint violations during this process
    pub fn decode_store(data: &[u8]) -> Result<(Self, Vec<ViolationsDelta>), BouncerError> {
        let (store, commits) = Store::decode(data)?;

        let query = ColnQuery::init(store.ir())?;
        let bouncer = Bouncer::new_from_store_query(store, query);

        let batch = bouncer.store.prepare_commits(commits, false)?;
        let violations = bouncer.begin_import().apply_batch(batch)?;

        if bouncer.store.begin_read().has_pending() {
            return Err(StoreError::Encode(
                coln_store::commit::error::CodecError::DataContentError(
                    "store snapshot contains commits that could not be applied".into(),
                ),
            )
            .into());
        }

        Ok((bouncer, violations))
    }
}
