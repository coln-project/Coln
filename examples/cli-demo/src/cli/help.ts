// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { Command } from "./arguments.js"

const overview = `coln-repo reads and updates synchronized Coln documents.

A Coln theory describes the entities and rules in a document. The compiler lowers
that theory to JSON IR. Read the IR, confirm inferred domain meanings with the
user, then query or execute a transaction.

Usage:
  coln-repo ir --document <coln-url> [--endpoint <url>] [-v]
  coln-repo query --document <coln-url> [--endpoint <url>] [-v]
  coln-repo exec --document <coln-url> [--endpoint <url>] [-v]
  coln-repo guide
  coln-repo install-skill [--dir <skills-root>]
  coln-repo help [command]

Commands:
  ir             Return the full compiled JSON IR
  query          Read a JavaScript expression from stdin and evaluate it
  exec           Read a JavaScript body from stdin and run one change transaction
  guide          Print the agent workflow for this tool (the coln-repo skill)
  install-skill  Install or update the coln-repo skill for agent harnesses

Options:
      --endpoint <url>  Sync server WebSocket URL, overriding SUBDUCTION_ENDPOINT
      --dir <path>      Skills root for install-skill (default ~/.agents/skills)
  -v, --verbose         Print connection and sync progress to stderr
  -h, --help            Print command help

Environment:
  SUBDUCTION_ENDPOINT   Sync server WebSocket URL, used when --endpoint is absent

The endpoint defaults to wss://coln.sync.inkandswitch.com. A document held
by a local relay is only reachable by pointing at that relay, for example
--endpoint ws://127.0.0.1:3030.

Agents: run "coln-repo guide" first. It explains how to read the IR, confirm
inferred meanings with the user, and interpret responses.

Every result, including an error, is JSON on stdout. JavaScript is trusted code
and runs with this Node process's capabilities.`

const commandHelp: Record<Command, string> = {
  guide: `Usage: coln-repo guide

Print the coln-repo skill body: the step-by-step workflow an agent should follow
to read a document's IR, confirm its interpretation, query, write, and read the
response. The same text is what install-skill installs.`,
  "install-skill": `Usage: coln-repo install-skill [--dir <skills-root>]

Copy the coln-repo skill directory to <skills-root>/coln-repo, creating it or
replacing it when the installed copy differs. The default root is
~/.agents/skills, which OpenCode reads. Other harnesses use their own root, for
example --dir ~/.claude/skills for Claude Code.

The response reports status "installed", "updated", or "unchanged".`,
  ir: `Usage: coln-repo ir --document <coln-url> [--endpoint <url>] [-v]

Return the document's compiled JSON IR without interpreting its schema.`,
  query: `Usage: coln-repo query --document <coln-url> [--endpoint <url>] [-v]

Evaluate a synchronous JavaScript expression with a read-only store in scope.
Read the expression from stdin and return its value in the JSON response.

Store methods:
  store.jsonIR()                 Return the IR JSON string
  store.scanTable(path)          Return all rows in an IR table
  store.rowById(path, rowRef)    Return one row, or undefined (rowRef = row.rowId.value)
  store.heads()                  Return current commit hashes

Example:
  printf 'store.scanTable("Records.Documents")' | coln-repo query --document coln:...`,
  exec: `Usage: coln-repo exec --document <coln-url> [--endpoint <url>] [-v]

Run a synchronous JavaScript script in one atomic Coln Repo change. The script
is read from stdin and receives the current transaction API as txn. Return a
JSON-serializable value to include it in the response.

Available operations:
  txn.add(path, values)          Add a row and return its tagged row reference

Values are tagged: { tag: "int", value } | { tag: "string", value } |
{ tag: "row_id", value: rowRef }.
txn.add returns a complete tagged row_id value; pass it directly to later adds.

Example:
  coln-repo exec --document coln:... <<'JS'
  const folder = txn.add("Records.Folders", [{ tag: "string", value: "Inbox" }])
  const document = txn.add("Records.Documents", [
    folder,
    { tag: "string", value: "Notes" },
  ])
  return { folder, document }
JS

The change is committed only if the script returns synchronously without error.
The CLI waits for the connected server to flush it before reporting success.`,
}

export function helpText(command?: Command): string {
  return command === undefined ? overview : commandHelp[command]
}
