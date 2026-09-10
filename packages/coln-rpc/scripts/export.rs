// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::Path;
use coln_rpc::api::*;
use coln_store::{table::WireValue, txn::TxnWireValue};
use specta::Types;
use specta_typescript::Typescript;

fn main() {
    let mut types = Types::default();

    types.register_mut::<WhereClause>();
    types.register_mut::<WireValue>();
    types.register_mut::<TxnWireValue>();
    types.register_mut::<Path>();

    Typescript::default()
        .export_to("./types.ts", &types, specta_serde::Format)
        .unwrap();
}
