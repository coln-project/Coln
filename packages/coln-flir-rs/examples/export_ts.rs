// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Exports the TypeScript definitions of the types crossing the JS boundary
//! to `coln-js-interface/src/types.ts` (or the path given as argument).
//!
//! Usage: `just export-ts [<out.ts>]` from this package, or
//! `cargo run -p coln-flir-rs --example export_ts [-- <out.ts>]`.

use coln_flir_rs::{
    engine::tx::{TxRowId, TxTuple},
    public::PublicTuple,
    query::WhereClause,
};
use specta::Types;
use specta_typescript::Typescript;

const HEADER: &str = "\
// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Regenerate with: just -f packages/coln-flir-rs/justfile export-ts";

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../coln-js-interface/src/types.ts"
        )
        .to_owned()
    });
    let mut types = Types::default();
    types.register_mut::<WhereClause>();
    types.register_mut::<PublicTuple>();
    types.register_mut::<TxTuple>();
    types.register_mut::<TxRowId>();
    Typescript::default()
        .header(HEADER)
        .export_to(&out, &types, specta_serde::Format)
        .unwrap();
}
