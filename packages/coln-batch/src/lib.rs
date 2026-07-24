// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Batch query engine for Coln (work in progress).
//!
//! This crate is built bottom-up:
//!
//! 1. deterministic test-data generators for e-matching-style join
//!    workloads ([`generate`]),
//! 2. Arrow IPC persistence for that test data ([`io`]),
//! 3. the [`table::SortedTable`] interface through which the engine reads
//!    relations, with an in-memory implementation built from Arrow data
//!    ([`table::ArrowSortedTable`]),
//! 4. conjunctive queries as data ([`query`], fixtures in [`fixtures`],
//!    a brute-force test oracle in [`reference`]),
//! 5. two executors over the same query representation: a binary
//!    hash-join chain ([`binary_join`]) and a worst-case-optimal generic
//!    join ([`generic_join`]), differential-tested against each other and
//!    against the oracle,
//! 6. (next) semi-naive recursion to a fixpoint.

pub mod binary_join;
pub mod fixtures;
pub mod generate;
pub mod generic_join;
pub mod io;
pub mod query;
pub mod reference;
pub mod relation;
pub mod rng;
pub mod table;
