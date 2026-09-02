// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnHandle } from "@coln-project/repo"

export function runIr(handle: ColnHandle): void {
  console.log(JSON.stringify(JSON.parse(handle.doc().jsonIR()), null, 2))
}
