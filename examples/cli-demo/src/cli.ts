#!/usr/bin/env node
// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { main } from "./cli/main.js"

ignoreBrokenPipe(process.stdout)
ignoreBrokenPipe(process.stderr)

let exitCode: number
try {
  exitCode = await main(process.argv.slice(2))
} catch (error) {
  console.log(JSON.stringify({
    protocolVersion: 0,
    ok: false,
    error: {
      code: "INTERNAL_ERROR",
      phase: "internal",
      message: error instanceof Error ? error.message : String(error),
      mutationState: "unknown",
    },
  }, null, 2))
  exitCode = 1
}

await Promise.all([drain(process.stdout), drain(process.stderr)])
process.exit(exitCode)

function drain(stream: NodeJS.WritableStream): Promise<void> {
  return new Promise(resolve => {
    stream.once("error", () => resolve())
    stream.write("", () => resolve())
  })
}

// A consumer such as `head` may close a pipe while main is still running.
function ignoreBrokenPipe(stream: NodeJS.WritableStream): void {
  stream.on("error", error => {
    if ((error as NodeJS.ErrnoException).code !== "EPIPE") throw error
  })
}
