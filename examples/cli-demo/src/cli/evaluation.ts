// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Console } from "node:console"
import type { ColnDocument, ColnTransaction } from "@coln-project/repo"

/** A program produced a value the JSON protocol cannot carry. */
export class InvalidResultError extends TypeError {
  constructor() {
    super("JavaScript result must be JSON-serializable")
    this.name = "InvalidResultError"
  }
}

export function evaluateQuery(store: ColnDocument, source: string): unknown {
  const evaluate = new Function("store", "console", `"use strict"; return (${source}\n)`) as (
    store: ColnDocument,
    console: Console,
  ) => unknown
  const result = evaluate(store, evaluationConsole)
  assertSynchronous(result)
  const json = jsonResult(result)
  return snapshotJsonResult(json)
}

export function evaluateExec(
  transaction: ColnTransaction,
  source: string,
): unknown {
  const evaluate = new Function("txn", "console", `"use strict";\n${source}`) as (
    transaction: ColnTransaction,
    console: Console,
  ) => unknown
  const result = evaluate(transaction, evaluationConsole)
  assertSynchronous(result)
  return jsonResult(result)
}

// Keep program logging away from the JSON protocol on stdout: every console
// method, including table/dir/group, writes to stderr.
const evaluationConsole = new Console({ stdout: process.stderr, stderr: process.stderr })

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

function jsonResult(value: unknown): unknown {
  if (value === undefined) return null
  assertJsonValue(value, new WeakSet())
  return value
}

export function snapshotJsonResult(value: unknown): unknown {
  return JSON.parse(JSON.stringify(value))
}

function assertJsonValue(value: unknown, ancestors: WeakSet<object>): void {
  if (value === null || typeof value === "string" || typeof value === "boolean") return
  if (typeof value === "number" && Number.isFinite(value)) return
  if (typeof value !== "object") throw new InvalidResultError()
  if (ancestors.has(value)) throw new InvalidResultError()

  const prototype = Object.getPrototypeOf(value)
  if (prototype !== Object.prototype && prototype !== Array.prototype && prototype !== null) {
    throw new InvalidResultError()
  }
  if (Object.getOwnPropertySymbols(value).length > 0) throw new InvalidResultError()

  ancestors.add(value)
  for (const descriptor of Object.values(Object.getOwnPropertyDescriptors(value))) {
    if (!descriptor.enumerable) continue
    if (!("value" in descriptor)) throw new InvalidResultError()
    assertJsonValue(descriptor.value, ancestors)
  }
  ancestors.delete(value)
}
