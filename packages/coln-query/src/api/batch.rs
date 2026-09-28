// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The batch backend's contract with the store, for coln-store to
//! implement: a [`RelationSource`] hands out a [`TableHandle`] per table,
//! and each handle serves [`SortedTable`]s in the column order the join
//! asks for. The requirements are documented on the traits, and
//! [`check_contract`] tests an implementation against them.

pub use crate::relational::batch::{
    ColId, Dictionary, Key, RelationSource, SortedTable, TableHandle,
};
pub use crate::relational::expr::SourceId;
pub use coln_batch::table::check_contract;
