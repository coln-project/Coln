// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { evaluateQuery } from "../evaluation.js"
import type { ColnHandle } from "@coln-project/repo"

export function runQuery(handle: ColnHandle, source: string): void {
  const result = evaluateQuery(handle.doc(), source)
  const json = JSON.stringify(result, null, 2)
  if (json === undefined) throw new TypeError("Query result is not JSON-serializable")
  console.log(json)
}
