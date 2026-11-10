// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{engine::tx::TxRowId, hash::CommitHash, public::PublicRowId};

pub trait Promote {
    /// Promoting pending ids to Wire ids, after successful transactions
    //  and also canonicalise them
    //  Will not check validity, callers is responsible for calling it with valid pending ids
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId>;

    fn promote_one(&self, pending_id: impl Into<TxRowId>, hash: CommitHash) -> PublicRowId {
        self.promote(std::iter::once(pending_id.into()), hash)
            .pop()
            .expect("ond id to promote")
    }
}
