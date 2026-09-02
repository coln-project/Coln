// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { expect, test } from "@playwright/test"
import { execFile } from "node:child_process"
import { promisify } from "node:util"
import { initSubduction, Repo } from "@automerge/automerge-repo"
import { colnDocType, type ColnSchema, type ColnTransaction } from "@coln-project/repo"

const execFileAsync = promisify(execFile)
const table = "Test.Items"
const schema = {
  entities: [{
    path: [["Test"], ["Items"]],
    value: {
      entityVariant: { tag: "table" },
      columns: [{
        path: [["value"]],
        type: { tag: "builtin", type: "builtinString" },
      }],
      primaryKey: null,
    },
  }],
  rules: [],
} satisfies ColnSchema

test("CLI exec flushes its change before exiting", async () => {
  await initSubduction()
  const endpoint = "ws://127.0.0.1:3031"
  const creator = new Repo({ subductionWebsocketEndpoints: [endpoint] })
  const verifier = new Repo({ subductionWebsocketEndpoints: [endpoint] })

  try {
    const handle = creator.create(schema, colnDocType)
    handle.change((transaction: ColnTransaction) => {
      transaction.add(table, [{ tag: "string", value: "before" }])
    })
    await creator.flush()

    const { stdout, stderr } = await execFileAsync(
      process.execPath,
      [
        "dist/cli.js",
        handle.url,
        "exec",
        `txn.add("${table}", [{ tag: "string", value: "from-cli" }])`,
        "--verbose",
      ],
      {
        cwd: process.cwd(),
        env: { ...process.env, SUBDUCTION_ENDPOINT: endpoint },
        timeout: 15_000,
      },
    )

    expect(stdout).toBe("")
    expect(stderr).toContain(`Connecting to ${endpoint}`)
    expect(stderr).toContain(`Synced to ${endpoint}`)

    const verified = await verifier.find(handle.url, colnDocType)
    await expect.poll(() =>
      verified.doc().scanTable(table)
        .map(row => row.values[0])
        .filter(value => value?.tag === "string")
        .map(value => value.value)
        .sort(),
    ).toEqual(["before", "from-cli"])
  } finally {
    await Promise.all([creator.shutdown(), verifier.shutdown()])
  }
})
