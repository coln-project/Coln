// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Scalar } from "#wasm-bodge/bindings";

export interface View {
  get(): Scalar
}

export interface Transaction extends View {
  set(v: Scalar): void;
}
