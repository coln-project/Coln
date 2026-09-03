// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { expect, test } from "@playwright/test"
import { spawn } from "node:child_process"
import { existsSync, mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { initSubduction, Repo } from "@automerge/automerge-repo"
import { colnDocType, type ColnSchema, type ColnTransaction } from "@coln-project/repo"

const endpoint = "ws://127.0.0.1:3031"
const peopleTable = "Scheduling.People"
const meetingsTable = "Scheduling.Meetings"
const schema = {
  entities: [
    {
      path: [["Scheduling"], ["People"]],
      value: {
        entityVariant: { tag: "table" },
        columns: [{
          path: [["name"]],
          type: { tag: "builtin", type: "builtinString" },
        }],
        primaryKey: null,
      },
    },
    {
      path: [["Scheduling"], ["Meetings"]],
      value: {
        entityVariant: { tag: "table" },
        columns: [
          {
            path: [["organizer"]],
            type: { tag: "rowId", path: [["Scheduling"], ["People"]] },
          },
          {
            path: [["attendee"]],
            type: { tag: "rowId", path: [["Scheduling"], ["People"]] },
          },
        ],
        primaryKey: null,
      },
    },
  ],
  rules: [],
} satisfies ColnSchema

interface CliResult {
  exitCode: number | null
  stdout: string
  stderr: string
}

function runCli(arguments_: string[], input = ""): Promise<CliResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["dist/cli.js", ...arguments_], {
      cwd: process.cwd(),
      env: process.env,
      timeout: 15_000,
    })
    let stdout = ""
    let stderr = ""
    child.stdout.setEncoding("utf8").on("data", chunk => { stdout += chunk })
    child.stderr.setEncoding("utf8").on("data", chunk => { stderr += chunk })
    child.on("error", reject)
    child.on("close", exitCode => resolve({ exitCode, stdout, stderr }))
    child.stdin.end(input)
  })
}

function documentArguments(command: string, documentUrl: string, verbose = false): string[] {
  return [
    command,
    "--document",
    documentUrl,
    "--endpoint",
    endpoint,
    ...(verbose ? ["--verbose"] : []),
  ]
}

test("CLI provides help, guide, and skill installation", async () => {
  const help = await runCli(["help", "exec"])
  expect(help.exitCode).toBe(0)
  expect(JSON.parse(help.stdout)).toMatchObject({
    protocolVersion: 0,
    ok: true,
    command: "help",
    help: expect.any(String),
  })

  const guide = await runCli(["guide"])
  expect(guide.exitCode).toBe(0)
  expect(JSON.parse(guide.stdout)).toMatchObject({
    protocolVersion: 0,
    ok: true,
    command: "guide",
    guide: expect.any(String),
  })

  const root = mkdtempSync(join(tmpdir(), "coln-skills-"))
  try {
    const installed = await runCli(["install-skill", "--dir", root])
    expect(installed.exitCode).toBe(0)
    expect(JSON.parse(installed.stdout)).toMatchObject({
      protocolVersion: 0,
      ok: true,
      command: "install-skill",
      skill: {
        name: "coln-repo",
        status: "installed",
        target: join(root, "coln-repo"),
        files: expect.arrayContaining(["SKILL.md"]),
      },
    })
    expect(existsSync(join(root, "coln-repo", "SKILL.md"))).toBe(true)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("CLI reads a store's IR and rows", async () => {
  await initSubduction()
  const creator = new Repo({ subductionWebsocketEndpoints: [endpoint] })

  try {
    const handle = creator.create(schema, colnDocType)
    handle.change((transaction: ColnTransaction) => {
      transaction.add(peopleTable, [{ tag: "string", value: "Ada" }])
    })
    await creator.flush()

    const ir = await runCli(documentArguments("ir", handle.url))
    expect(ir.exitCode).toBe(0)
    expect(ir.stderr).toBe("")
    const irResponse = JSON.parse(ir.stdout)
    expect(irResponse).toMatchObject({
      protocolVersion: 0,
      ok: true,
      command: "ir",
      document: { url: handle.url, endpoint, heads: expect.any(Array) },
      ir: schema,
    })
    expect(irResponse.document.heads.length).toBeGreaterThan(0)

    const query = await runCli(
      documentArguments("query", handle.url),
      `store.scanTable("${peopleTable}").map(row => row.values[0].value)`,
    )
    expect(query.exitCode).toBe(0)
    const queryResponse = JSON.parse(query.stdout)
    expect(queryResponse).toMatchObject({
      protocolVersion: 0,
      ok: true,
      command: "query",
      document: { url: handle.url, endpoint, heads: expect.any(Array) },
      result: ["Ada"],
    })
    expect(queryResponse.document.heads.length).toBeGreaterThan(0)
  } finally {
    await creator.shutdown()
  }
})

test("CLI commits dependent rows in one transaction", async () => {
  await initSubduction()
  const creator = new Repo({ subductionWebsocketEndpoints: [endpoint] })
  const verifier = new Repo({ subductionWebsocketEndpoints: [endpoint] })

  try {
    const handle = creator.create(schema, colnDocType)
    await creator.flush()

    const result = await runCli(documentArguments("exec", handle.url, true), `
      const organizer = txn.add("${peopleTable}", [{ tag: "string", value: "Ada" }])
      const attendee = txn.add("${peopleTable}", [{ tag: "string", value: "Lin" }])
      const meeting = txn.add("${meetingsTable}", [organizer, attendee])
      console.log("scheduled")
      return { organizer, attendee, meeting }
    `)
    expect(result.exitCode).toBe(0)
    expect(result.stderr).toContain("scheduled")
    const response = JSON.parse(result.stdout)
    expect(response).toMatchObject({
      protocolVersion: 0,
      ok: true,
      command: "exec",
      document: {
        url: handle.url,
        endpoint,
        headsBefore: expect.any(Array),
        headsAfter: expect.any(Array),
      },
      sync: { status: "flushed" },
    })
    expect(response.document.headsBefore.length).toBeGreaterThan(0)
    expect(response.document.headsAfter.length).toBeGreaterThan(0)
    expect(response.document.headsAfter).not.toEqual(response.document.headsBefore)
    expect(response.result.organizer.value.existing.commit).toBe(response.document.headsAfter[0])
    expect(response.result.attendee.value.existing.commit).toBe(response.document.headsAfter[0])
    expect(response.result.meeting.value.existing.commit).toBe(response.document.headsAfter[0])

    const verified = await verifier.find(handle.url, colnDocType)
    await expect.poll(() => verified.doc().scanTable(meetingsTable).length).toBe(1)
    expect(verified.doc().scanTable(peopleTable)).toHaveLength(2)
    expect(verified.doc().scanTable(meetingsTable)[0].values).toEqual([
      response.result.organizer,
      response.result.attendee,
    ])
  } finally {
    await Promise.all([creator.shutdown(), verifier.shutdown()])
  }
})

test("CLI reports a failed transaction without changing the store", async () => {
  await initSubduction()
  const creator = new Repo({ subductionWebsocketEndpoints: [endpoint] })

  try {
    const handle = creator.create(schema, colnDocType)
    await creator.flush()

    const result = await runCli(
      documentArguments("exec", handle.url),
      'txn.add("Scheduling.Missing", [])',
    )
    expect(result.exitCode).not.toBe(0)
    const response = JSON.parse(result.stdout)
    expect(response).toMatchObject({
      protocolVersion: 0,
      ok: false,
      command: "exec",
      document: {
        url: handle.url,
        endpoint,
        headsBefore: expect.any(Array),
        headsAfter: expect.any(Array),
      },
      error: {
        code: "TRANSACTION_FAILED",
        message: expect.any(String),
        mutationState: "not_applied",
      },
    })
    expect(response.error.message.length).toBeGreaterThan(0)
    expect(response.document.headsAfter).toEqual(response.document.headsBefore)

    const query = await runCli(
      documentArguments("query", handle.url),
      `store.scanTable("${peopleTable}").length`,
    )
    expect(query.exitCode).toBe(0)
    expect(JSON.parse(query.stdout)).toMatchObject({
      protocolVersion: 0,
      ok: true,
      command: "query",
      document: { url: handle.url, endpoint, heads: response.document.headsBefore },
      result: 0,
    })
  } finally {
    await creator.shutdown()
  }
})
