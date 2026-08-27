// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT
// 
import * as ColnSet from "./ColnSet"

import { Scalar, StoreHandle, RowView, TransactionHandle, getRowRef } from "#wasm-bodge/bindings"
import { Tuple, tupleEqual } from "./tuple"

export class View {
  store: StoreHandle;
  query: string;
  reconstruct: (result: Tuple) => Value;
  params: Tuple;
}
