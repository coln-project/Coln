// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { describe, expect, it } from "vitest"
import { snapshot } from "../src/tools/editor/snapshot.ts"

describe("snapshot", () => {
  it("copies plain values and marks cycles", () => {
    const value: Record<string, unknown> = { answer: 42 }
    value.self = value
    expect(snapshot(value)).toEqual({ answer: 42, self: { $type: "circular" } })
  })

  it("does not invoke accessors or retain class instances", () => {
    let accessed = false
    const value = Object.defineProperty({}, "secret", {
      enumerable: true,
      get() {
        accessed = true
        return 1
      },
    })
    expect(snapshot(value)).toEqual({ secret: { $type: "accessor" } })
    expect(accessed).toBe(false)
    expect(snapshot(new Date(0))).toEqual({ $type: "instance", value: "Date" })
  })

  it("contains objects that throw during inspection", () => {
    const { proxy, revoke } = Proxy.revocable({}, {})
    revoke()
    expect(snapshot(proxy)).toEqual({ $type: "uninspectable" })
  })

  it("preserves an own __proto__ property", () => {
    const value = Object.create(null) as Record<string, unknown>
    value.__proto__ = "data"
    const result = snapshot(value)
    expect(Object.prototype.hasOwnProperty.call(result, "__proto__")).toBe(true)
    expect((result as Record<string, unknown>).__proto__).toBe("data")
  })
})
