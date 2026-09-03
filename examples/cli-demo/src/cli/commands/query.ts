// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { evaluateQuery } from "../evaluation.js"
import type { ColnHandle } from "@coln-project/repo"

export function runQuery(handle: ColnHandle, source: string): unknown {
  return evaluateQuery(handle.doc(), source)
}
