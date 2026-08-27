// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Scalar, scalarEqual } from "#wasm-bodge/bindings";

export type Tuple = Scalar[];

export function tupleEqual(t0: Tuple, t1: Tuple): boolean {
  if (t0.length != t1.length) {
    return false;
  }
  for (var i = 0; i < t0.length; i += 1) {
    if (!scalarEqual(t0[i], t1[i])) {
      return false;
    }
  }
  return true;
}
