// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { evaluateExec, snapshotJsonResult } from "../evaluation.js"
import type { ColnHandle } from "@coln-project/repo"

export function runExec(handle: ColnHandle, source: string): unknown {
  let result: unknown = null

  handle.change(transaction => {
    result = evaluateExec(transaction, source)
  })

  // Commit resolves pending row references in place; snapshot only afterwards.
  return snapshotJsonResult(result)
}
