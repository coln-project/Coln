// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { evaluateExec } from "../evaluation.js"
import type { ColnHandle } from "@coln-project/repo"

export function runExec(handle: ColnHandle, source: string): void {
  handle.change(transaction => evaluateExec(transaction, source))
}
