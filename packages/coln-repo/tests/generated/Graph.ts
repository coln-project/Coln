// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as runtime from "@coln-project/runtime";

export interface View {
  V: runtime.ColnSet.View;
  E: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  V: runtime.ColnSet.Transaction;
  E: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
}