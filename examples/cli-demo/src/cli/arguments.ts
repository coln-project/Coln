// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

export type DocumentCommand = "ir" | "query" | "exec"
export type Command = DocumentCommand | "guide" | "install-skill"

export type ParsedArguments =
  | { kind: "help"; command?: Command }
  | { kind: "guide" }
  | { kind: "install-skill"; directory?: string }
  | {
      kind: "command"
      command: DocumentCommand
      documentUrl: string
      verbose: boolean
      endpoint?: string
    }

const documentCommands: DocumentCommand[] = ["ir", "query", "exec"]
const commands: Command[] = [...documentCommands, "guide", "install-skill"]

export function parseArguments(arguments_: string[]): ParsedArguments {
  const positional: string[] = []
  let verbose = false
  let help = false
  let documentUrl: string | undefined
  let endpoint: string | undefined
  let directory: string | undefined

  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index]
    if (argument === "-v" || argument === "--verbose") verbose = true
    else if (argument === "-h" || argument === "--help") help = true
    else if (argument.startsWith("--document=")) {
      documentUrl = argument.slice("--document=".length)
    } else if (argument === "--document") {
      documentUrl = optionValue(arguments_, ++index, "--document requires a Coln URL")
    } else if (argument.startsWith("--endpoint=")) {
      endpoint = argument.slice("--endpoint=".length)
      if (endpoint === "") throw new Error("--endpoint requires a WebSocket URL")
    } else if (argument === "--endpoint") {
      endpoint = optionValue(arguments_, ++index, "--endpoint requires a WebSocket URL")
    } else if (argument.startsWith("--dir=")) {
      directory = argument.slice("--dir=".length)
      if (directory === "") throw new Error("--dir requires a directory path")
    } else if (argument === "--dir") {
      directory = optionValue(arguments_, ++index, "--dir requires a directory path")
    } else if (argument.startsWith("-")) throw new Error(`Unknown option: ${argument}`)
    else positional.push(argument)
  }

  const explicitHelp = help || positional[0] === "help"
  if (explicitHelp || arguments_.length === 0) {
    const command = positional[0] === "help" ? positional[1] : positional[0]
    if (command !== undefined && !isCommand(command)) {
      throw new Error(`Unknown command: ${command}`)
    }
    return { kind: "help", command }
  }

  const [command, ...extra] = positional
  if (command === undefined) throw new Error("Missing command")
  if (!isCommand(command)) throw new Error(`Unknown command: ${command}`)
  if (extra.length > 0) throw new Error(`Unexpected argument: ${extra[0]}`)

  if (command === "guide" || command === "install-skill") {
    if (documentUrl !== undefined) throw new Error(`--document is not used by ${command}`)
    if (endpoint !== undefined) throw new Error(`--endpoint is not used by ${command}`)
    if (command === "guide") {
      if (directory !== undefined) throw new Error("--dir is not used by guide")
      return { kind: "guide" }
    }
    return { kind: "install-skill", directory }
  }

  if (directory !== undefined) throw new Error(`--dir is not used by ${command}`)
  if (documentUrl === undefined || documentUrl === "") {
    throw new Error("--document requires a Coln URL")
  }

  return { kind: "command", command, documentUrl, verbose, endpoint }
}

function isCommand(value: string | undefined): value is Command {
  return value !== undefined && (commands as string[]).includes(value)
}

function optionValue(arguments_: string[], index: number, message: string): string {
  const value = arguments_[index]
  if (value === undefined || value.startsWith("-")) throw new Error(message)
  return value
}
