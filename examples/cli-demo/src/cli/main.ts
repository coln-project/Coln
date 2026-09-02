// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { parseArguments } from "./arguments.js"
import { runExec } from "./commands/exec.js"
import { runIr } from "./commands/ir.js"
import { runQuery } from "./commands/query.js"
import { helpText } from "./help.js"
import { defaultEndpoint, flush, openDocument } from "./repository.js"

export async function main(arguments_: string[]): Promise<number> {
  const argumentsResult = parseArguments(arguments_)
  if (argumentsResult.kind === "help") {
    console.log(helpText(argumentsResult.command))
    return 0
  }

  const endpoint = process.env.SUBDUCTION_ENDPOINT ?? defaultEndpoint
  const source = argumentsResult.source === "-"
    ? await readStdin()
    : argumentsResult.source
  const { repo, handle } = await openDocument(
    argumentsResult.documentUrl,
    endpoint,
    argumentsResult.verbose,
  )

  try {
    switch (argumentsResult.command) {
      case "ir":
        runIr(handle)
        break
      case "query":
        runQuery(handle, source!)
        break
      case "exec":
        runExec(handle, source!)
        await flush(repo, endpoint, argumentsResult.verbose)
        break
    }
    return 0
  } finally {
    await repo.shutdown()
  }
}

async function readStdin(): Promise<string> {
  process.stdin.setEncoding("utf8")
  let source = ""
  for await (const chunk of process.stdin) source += chunk
  return source
}
