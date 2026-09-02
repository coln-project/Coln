#!/usr/bin/env node
// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { main } from "./cli/main.js"

let exitCode: number
try {
  exitCode = await main(process.argv.slice(2))
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error))
  exitCode = 1
}

await Promise.all([drain(process.stdout), drain(process.stderr)])
process.exit(exitCode)

function drain(stream: NodeJS.WritableStream): Promise<void> {
  return new Promise((resolve, reject) => {
    stream.write("", error => error ? reject(error) : resolve())
  })
}
