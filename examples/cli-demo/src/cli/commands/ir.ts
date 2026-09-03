// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnHandle } from "@coln-project/repo"

export function runIr(handle: ColnHandle): unknown {
  return JSON.parse(handle.doc().jsonIR())
}
