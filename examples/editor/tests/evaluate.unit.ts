// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnHandle } from "@coln-project/repo"
import { describe, expect, it } from "vitest"
import { evaluate } from "../src/lib/evaluate.ts"

const handle = { marker: "handle" } as unknown as ColnHandle

describe("evaluate", () => {
  it("awaits programs and captures console output", async () => {
    const result = await evaluate(
      'console.log("ready", 2); await Promise.resolve(); return handle.marker',
      handle,
    )
    expect(result).toMatchObject({
      ok: true,
      result: "handle",
      console: [{ level: "log", values: ["ready", 2] }],
    })
  })

  it("separates syntax and runtime failures", async () => {
    expect(await evaluate("return )", handle)).toMatchObject({
      ok: false,
      phase: "syntax",
    })
    expect(await evaluate('throw new Error("broken")', handle)).toMatchObject({
      ok: false,
      phase: "runtime",
      error: { $type: "error", message: "broken" },
    })
  })
})
