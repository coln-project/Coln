// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

export type Command = "ir" | "query" | "exec"

export type ParsedArguments =
  | { kind: "help"; command?: Command }
  | {
      kind: "command"
      command: Command
      documentUrl: string
      source?: string
      verbose: boolean
    }

export function parseArguments(arguments_: string[]): ParsedArguments {
  const positional: string[] = []
  let verbose = false
  let help = false
  let optionsEnded = false

  for (const argument of arguments_) {
    if (!optionsEnded && argument === "--") optionsEnded = true
    else if (!optionsEnded && (argument === "-v" || argument === "--verbose")) verbose = true
    else if (!optionsEnded && (argument === "-h" || argument === "--help")) help = true
    else positional.push(argument)
  }

  if (positional.length === 0 || positional[0] === "help") {
    const command = positional[1]
    if (command !== undefined && !isCommand(command)) {
      throw new Error(`Unknown command: ${command}`)
    }
    return { kind: "help", command }
  }

  const [documentUrl, command, source, ...extra] = positional
  if (!isCommand(command)) {
    throw new Error(command === undefined ? "Missing command" : `Unknown command: ${command}`)
  }
  if (help) return { kind: "help", command }
  if (extra.length > 0) throw new Error(`Unexpected argument: ${extra[0]}`)
  if (command !== "ir" && source === undefined) {
    throw new Error(`${command} requires JavaScript or - for stdin`)
  }
  if (command === "ir" && source !== undefined) throw new Error(`Unexpected argument: ${source}`)

  return { kind: "command", command, documentUrl, source, verbose }
}

function isCommand(value: string | undefined): value is Command {
  return value === "ir" || value === "query" || value === "exec"
}
