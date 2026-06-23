// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod dto;
mod error;
mod handles;

pub use handles::{CommitResult, StoreHandle, TransactionHandle};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}
