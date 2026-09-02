// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnDocument, ColnTransaction } from "@coln-project/repo"

export function evaluateQuery(store: ColnDocument, source: string): unknown {
  const evaluate = new Function("store", `"use strict"; return (${source}\n)`) as (
    store: ColnDocument,
  ) => unknown
  const result = evaluate(store)
  assertSynchronous(result)
  return result
}

export function evaluateExec(
  transaction: ColnTransaction,
  source: string,
): void {
  const evaluate = new Function("txn", `"use strict";\n${source}`) as (
    transaction: ColnTransaction,
  ) => unknown
  assertSynchronous(evaluate(transaction))
}

function assertSynchronous(value: unknown): void {
  if (
    (typeof value === "object" || typeof value === "function")
    && value !== null
    && "then" in value
    && typeof value.then === "function"
  ) {
    throw new TypeError("JavaScript must complete synchronously")
  }
}
