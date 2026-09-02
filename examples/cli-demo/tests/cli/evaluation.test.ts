// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict"
import test from "node:test"
import type { ColnDocument, ColnTransaction } from "@coln-project/repo"
import { evaluateExec, evaluateQuery } from "../../src/cli/evaluation.js"

test("query evaluates against a read-only store", () => {
  const document = {
    heads: () => [],
    jsonIR: () => "{}",
    rowById: () => undefined,
    scanTable: () => [{ values: [{ tag: "string", value: "one" }] }],
  } as unknown as ColnDocument

  const result = evaluateQuery(
    document,
    `({ keys: Object.keys(store).sort(), values: store.scanTable("Test.Items").map(row => row.values[0].value) })`,
  )

  assert.deepEqual(result, {
    keys: ["heads", "jsonIR", "rowById", "scanTable"],
    values: ["one"],
  })
})

test("exec writes through its transaction", () => {
  const values: string[] = []
  const transaction = {
    add: (_path: string, rows: { value: string }[]) => values.push(rows[0].value),
  } as unknown as ColnTransaction

  evaluateExec(transaction, `txn.add("Test.Items", [{ tag: "string", value: "one" }])`)
  assert.deepEqual(values, ["one"])
})

test("exec receives the current transaction API", () => {
  const transaction = { add: () => undefined } as ColnTransaction

  evaluateExec(transaction, `
    if (JSON.stringify(Object.keys(txn)) !== '["add"]') throw new Error("unexpected API")
    txn.add("Test.Items", [{ tag: "string", value: "one" }])
  `)
})

test("query and exec reject promises", () => {
  const document = {
    heads: () => [],
    jsonIR: () => "{}",
    rowById: () => undefined,
    scanTable: () => [],
  } as ColnDocument
  assert.throws(() => evaluateQuery(document, "Promise.resolve(1)"), /synchronously/)

  const transaction = { add: () => undefined } as ColnTransaction
  assert.throws(
    () => evaluateExec(transaction, "return Promise.resolve()"),
    /synchronously/,
  )
})
