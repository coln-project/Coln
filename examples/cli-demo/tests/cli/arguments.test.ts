// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict"
import test from "node:test"
import { parseArguments } from "../../src/cli/arguments.js"

test("parses commands and flags in any position", () => {
  assert.deepEqual(parseArguments(["-v", "automerge:abc", "query", "1 + 1"]), {
    kind: "command",
    command: "query",
    documentUrl: "automerge:abc",
    source: "1 + 1",
    verbose: true,
  })
})

test("parses command help", () => {
  assert.deepEqual(parseArguments(["automerge:abc", "exec", "--help"]), {
    kind: "help",
    command: "exec",
  })
  assert.deepEqual(parseArguments(["help", "query"]), {
    kind: "help",
    command: "query",
  })
})

test("double dash allows JavaScript matching an option", () => {
  assert.deepEqual(parseArguments(["automerge:abc", "query", "--", "--verbose"]), {
    kind: "command",
    command: "query",
    documentUrl: "automerge:abc",
    source: "--verbose",
    verbose: false,
  })
})

test("requires JavaScript for query and exec", () => {
  assert.throws(() => parseArguments(["automerge:abc", "exec"]), /requires JavaScript/)
})

test("rejects unknown commands and extra arguments", () => {
  assert.throws(() => parseArguments(["automerge:abc", "nope"]), /Unknown command/)
  assert.throws(
    () => parseArguments(["automerge:abc", "query", "1", "2"]),
    /Unexpected argument/,
  )
})
