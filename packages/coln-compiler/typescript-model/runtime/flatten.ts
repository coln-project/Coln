// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { WireTuple } from "./store";

export interface Adaptor<T> {
  flatten: (value: T) => WireTuple,
  reconstruct: (tuple: WireTuple) => T
}
