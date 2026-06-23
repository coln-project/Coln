// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Value } from "#wasm-bodge/bindings";

export interface View {
  get(): Value
}

export interface Transaction extends View {
  set(v: Value): void;
}
