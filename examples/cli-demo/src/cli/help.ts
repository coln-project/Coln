// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { Command } from "./arguments.js"

const overview = `coln-repo reads and updates synchronized Coln documents.

A Coln theory describes the entities and rules in a document. The compiler lowers
that theory to JSON IR. The runtime Store uses IR table paths and tagged values;
use "ir" first to discover those paths and column types.

Usage:
  coln-repo <automerge-url> ir [-v]
  coln-repo <automerge-url> query <javascript|-> [-v]
  coln-repo <automerge-url> exec <javascript|-> [-v]
  coln-repo help [command]

Commands:
  ir      Print compiled IR as formatted JSON
  query   Evaluate a read-only expression and print its result as JSON
  exec    Run a synchronous script in one change transaction

Options:
  -v, --verbose  Print connection and sync progress to stderr
  -h, --help     Print command help

Environment:
  SUBDUCTION_ENDPOINT  Sync server WebSocket URL

JavaScript is trusted code and runs with this Node process's capabilities.`

const commandHelp: Record<Command, string> = {
  ir: `Usage: coln-repo <automerge-url> ir [-v]

Print the document's compiled Coln IR as formatted JSON.`,
  query: `Usage: coln-repo <automerge-url> query <expression|-> [-v]

Evaluate a synchronous JavaScript expression with a read-only store in scope.
The result is JSON-encoded on stdout. Use - to read the expression from stdin.

Store methods:
  store.jsonIR()                 Return the IR JSON string
  store.scanTable(path)          Return all rows in an IR table
  store.rowById(path, rowRef)    Return one row, or undefined
  store.heads()                  Return current commit hashes

Example:
  coln-repo automerge:... query 'store.scanTable("GraphRealm.V")'`,
  exec: `Usage: coln-repo <automerge-url> exec <script|-> [-v]

Run a synchronous JavaScript script in one atomic Coln Repo change. The script
receives an add-only transaction named txn. Use - to read the script from stdin.
Successful execution writes no stdout.

Available operations:
  txn.add(path, values)          Add a row and return its tagged row reference

Example:
  coln-repo automerge:... exec '
    const a = txn.add("GraphRealm.V", [])
    const b = txn.add("GraphRealm.V", [])
    txn.add("GraphRealm.E", [a, b])
  '

The change is committed only if the script returns synchronously without error.
The CLI waits for the connected server to flush it before exiting successfully.`,
}

export function helpText(command?: Command): string {
  return command === undefined ? overview : commandHelp[command]
}
