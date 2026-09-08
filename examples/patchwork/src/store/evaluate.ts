// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnHandle, RealmBindings } from "@coln-project/repo"
import { snapshot, type Snapshot } from "./snapshot.ts"

export type ConsoleLevel = "debug" | "info" | "log" | "warn" | "error"

export interface ConsoleEntry {
  level: ConsoleLevel
  values: Snapshot[]
}

export interface EvaluationSuccess {
  ok: true
  result: Snapshot
  console: ConsoleEntry[]
  durationMs: number
}

export interface EvaluationFailure {
  ok: false
  phase: "syntax" | "runtime"
  error: Snapshot
  console: ConsoleEntry[]
  durationMs: number
}

export type Evaluation = EvaluationSuccess | EvaluationFailure

type AsyncProgram = (
  handle: unknown,
  console: Pick<Console, ConsoleLevel>,
) => Promise<unknown>

const AsyncFunction = Object.getPrototypeOf(async function () {})
  .constructor as new (...parameters: string[]) => AsyncProgram

export async function evaluate<Bindings extends RealmBindings | undefined>(
  source: string,
  handle: ColnHandle<Bindings>,
): Promise<Evaluation> {
  const entries: ConsoleEntry[] = []
  let active = true
  const capturedConsole = Object.fromEntries(
    (["debug", "info", "log", "warn", "error"] as const).map((level) => [
      level,
      (...values: unknown[]) => {
        if (active)
          entries.push({
            level,
            values: values.map((value) => snapshot(value)),
          })
      },
    ]),
  ) as Pick<Console, ConsoleLevel>

  let program: AsyncProgram
  const started = performance.now()
  try {
    program = new AsyncFunction(
      "handle",
      "console",
      `"use strict";\n${source}\n//# sourceURL=coln-patchwork-repl.js`,
    )
  } catch (error) {
    active = false
    return {
      ok: false,
      phase: "syntax",
      error: snapshot(error),
      console: entries,
      durationMs: performance.now() - started,
    }
  }

  try {
    const result = await program(handle, capturedConsole)
    active = false
    return {
      ok: true,
      result: snapshot(result),
      console: entries,
      durationMs: performance.now() - started,
    }
  } catch (error) {
    active = false
    return {
      ok: false,
      phase: "runtime",
      error: snapshot(error),
      console: entries,
      durationMs: performance.now() - started,
    }
  }
}
