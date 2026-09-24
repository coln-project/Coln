// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { WireTuple, LiveTuple } from "./value.js";

export interface Adaptor<T> {
  flatten: (value: T) => LiveTuple,
  reconstruct: (tuple: WireTuple) => T
}
