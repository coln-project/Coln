// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Value, RowView  } from "#wasm-bodge/bindings";

export interface View {
  has(x: Value): boolean;
  values(): Iterator<RowView>;
}

export interface Transaction extends View {
  add(): Value;
}
