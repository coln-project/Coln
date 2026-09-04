// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  loadSource,
  saveSource,
  starterSource,
} from "../src/lib/source-storage.ts"

const values = new Map<string, string>()

beforeEach(() => {
  values.clear()
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  })
})

afterEach(() => vi.unstubAllGlobals())

describe("source storage", () => {
  it("stores source independently by document URL", () => {
    expect(loadSource("automerge:one")).toBe(starterSource)
    saveSource("automerge:one", "return 1")
    saveSource("automerge:two", "return 2")
    expect(loadSource("automerge:one")).toBe("return 1")
    expect(loadSource("automerge:two")).toBe("return 2")
  })
})
