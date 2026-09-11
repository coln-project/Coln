// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { join } from "node:path"
import { parseArguments, type Command } from "./arguments.js"
import { runExec } from "./commands/exec.js"
import { runIr } from "./commands/ir.js"
import { runQuery } from "./commands/query.js"
import { InvalidResultError } from "./evaluation.js"
import { helpText } from "./help.js"
import { defaultEndpoint, flush, openDocument } from "./repository.js"
import { defaultSkillInstallRoot, installSkill, readGuide, skillName } from "./skill.js"

const protocolVersion = 0

export async function main(arguments_: string[]): Promise<number> {
  let argumentsResult
  try {
    argumentsResult = parseArguments(arguments_)
  } catch (error) {
    writeJson(failure(undefined, "INVALID_ARGUMENTS", "arguments", error, "not_applied"))
    return 2
  }

  if (argumentsResult.kind === "help") {
    writeJson({
      protocolVersion,
      ok: true,
      command: "help",
      help: helpText(argumentsResult.command),
    })
    return 0
  }

  if (argumentsResult.kind === "guide") {
    try {
      writeJson({ ...success("guide"), guide: readGuide() })
      return 0
    } catch (error) {
      writeJson(failure("guide", "GUIDE_FAILED", "read", error, "not_applied"))
      return 4
    }
  }

  if (argumentsResult.kind === "install-skill") {
    const root = argumentsResult.directory ?? defaultSkillInstallRoot
    try {
      const installed = installSkill(root)
      writeJson({ ...success("install-skill"), skill: { name: skillName, ...installed } })
      return 0
    } catch (error) {
      writeJson({
        ...failure("install-skill", "SKILL_INSTALL_FAILED", "install", error, "not_applied"),
        skill: { name: skillName, target: join(root, skillName) },
      })
      return 4
    }
  }

  const { command, documentUrl, verbose } = argumentsResult
  const endpoint = argumentsResult.endpoint ?? process.env.SUBDUCTION_ENDPOINT ?? defaultEndpoint
  let source: string | undefined
  if (command === "query" || command === "exec") {
    if (process.stdin.isTTY) {
      writeJson(failure(command, "PROGRAM_REQUIRED", "arguments", new Error(
        `${command} reads JavaScript from stdin; pipe or redirect a program into it`,
      ), "not_applied"))
      return 2
    }
    source = await readStdin()
    if (source.trim() === "") {
      writeJson(failure(command, "PROGRAM_REQUIRED", "arguments", new Error(
        `${command} requires JavaScript on stdin`,
      ), "not_applied"))
      return 2
    }
  }

  let opened
  try {
    opened = await openDocument(documentUrl, endpoint, verbose)
  } catch (error) {
    writeJson({
      ...failure(command, "DOCUMENT_OPEN_FAILED", "open", error, "not_applied"),
      document: { url: documentUrl, endpoint },
    })
    return 3
  }

  const { repo, handle } = opened
  try {
    if (command === "ir") {
      let heads: string[] | undefined
      try {
        heads = handle.doc().heads()
        writeJson({
          ...success(command),
          document: documentState(documentUrl, endpoint, heads),
          ir: runIr(handle),
        })
        return 0
      } catch (error) {
        writeJson({
          ...failure(command, "IR_FAILED", "read", error, "not_applied"),
          document: heads === undefined
            ? { url: documentUrl, endpoint }
            : documentState(documentUrl, endpoint, heads),
        })
        return 4
      }
    }

    if (command === "query") {
      let heads: string[] | undefined
      try {
        heads = handle.doc().heads()
        const result = runQuery(handle, source!)
        writeJson({
          ...success(command),
          document: documentState(documentUrl, endpoint, heads),
          result,
        })
        return 0
      } catch (error) {
        writeJson({
          ...failure(command, "QUERY_FAILED", "evaluate", error, "not_applied"),
          document: heads === undefined
            ? { url: documentUrl, endpoint }
            : documentState(documentUrl, endpoint, heads),
        })
        return 4
      }
    }

    const headsBefore = handle.doc().heads()
    let result: unknown
    try {
      result = runExec(handle, source!)
    } catch (error) {
      const headsAfter = handle.doc().heads()
      // Three failure shapes: the transaction rejected the program (rolled back),
      // the program's return value is not JSON (rolled back before commit), or
      // snapshotting the value after commit failed (committed locally).
      const applied = !sameHeads(headsBefore, headsAfter)
      const invalidResult = error instanceof InvalidResultError
      writeJson({
        ...failure(
          command,
          applied ? "RESULT_FAILED" : invalidResult ? "RESULT_INVALID" : "TRANSACTION_FAILED",
          applied || invalidResult ? "result" : "transaction",
          error,
          applied ? "applied_locally" : "not_applied",
        ),
        document: transactionState(documentUrl, endpoint, headsBefore, headsAfter),
      })
      return 5
    }

    const headsAfter = handle.doc().heads()
    try {
      await flush(repo, endpoint, verbose)
    } catch (error) {
      writeJson({
        ...failure(command, "SYNC_FAILED", "flush", error, "applied_locally"),
        document: transactionState(documentUrl, endpoint, headsBefore, headsAfter),
        result,
      })
      return 6
    }

    writeJson({
      ...success(command),
      document: transactionState(documentUrl, endpoint, headsBefore, headsAfter),
      sync: { status: "flushed" },
      result,
    })
    return 0
  } finally {
    await repo.shutdown().catch(error => console.error(errorMessage(error)))
  }
}

function success(command: Command): object {
  return { protocolVersion, ok: true, command }
}

function failure(
  command: Command | undefined,
  code: string,
  phase: string,
  error: unknown,
  mutationState: "not_applied" | "applied_locally",
): object {
  return {
    protocolVersion,
    ok: false,
    ...(command === undefined ? {} : { command }),
    error: {
      code,
      phase,
      message: errorMessage(error),
      mutationState,
    },
  }
}

function documentState(url: string, endpoint: string, heads: string[]): object {
  return { url, endpoint, heads }
}

function transactionState(
  url: string,
  endpoint: string,
  headsBefore: string[],
  headsAfter: string[],
): object {
  return { url, endpoint, headsBefore, headsAfter }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function sameHeads(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every(head => right.includes(head))
}

function writeJson(value: unknown): void {
  console.log(JSON.stringify(value, null, 2))
}

async function readStdin(): Promise<string> {
  process.stdin.setEncoding("utf8")
  let source = ""
  for await (const chunk of process.stdin) source += chunk
  return source
}
