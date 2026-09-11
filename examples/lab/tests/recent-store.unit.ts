// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  loadRecentStore,
  saveRecentStore,
} from "../src/tools/editor/recent-store.ts"

const values = new Map<string, string>()

beforeEach(() => {
  values.clear()
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  })
})

afterEach(() => vi.unstubAllGlobals())

describe("recent store storage", () => {
  it("persists the latest store", () => {
    expect(loadRecentStore()).toBe("")
    saveRecentStore("coln:first")
    saveRecentStore("coln:latest")
    expect(loadRecentStore()).toBe("coln:latest")
  })

  it("tolerates unavailable storage", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => { throw new Error("unavailable") },
      setItem: () => { throw new Error("unavailable") },
    })
    expect(loadRecentStore()).toBe("")
    expect(() => saveRecentStore("coln:store")).not.toThrow()
  })
})
