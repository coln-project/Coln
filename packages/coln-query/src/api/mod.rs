// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module defines the public API of the query engine intended to be mainly
//! used by `coln-store`. See the other module level documentations.

// Receiving `coln-compiler`'s IR is blocked by its stabiliztion and hence still
// missing in here.

pub mod deltas;
pub mod schema;
pub mod store;
pub mod transaction;
pub mod violations;
