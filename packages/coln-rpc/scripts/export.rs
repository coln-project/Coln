// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_rpc::api::*;
use specta::Types;
use specta_typescript::Typescript;

fn main() {
    let mut types = Types::default();

    types.register_mut::<Query>();
    types.register_mut::<QueryResponse>();

    Typescript::default()
        .export_to("./types.ts", &types, specta_serde::Format)
        .unwrap();
}
