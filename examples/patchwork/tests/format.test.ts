// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { describe, expect, it } from "vitest"
import { displayRowRef, displayValue } from "../src/store/format.ts"

describe("value formatting", () => {
  it("formats primitive values", () => {
    expect(displayValue({ tag: "string", value: "Ada" })).toMatchObject({
      compact: "Ada",
      kind: "string",
    })
    expect(displayValue({ tag: "int", value: 37 })).toMatchObject({
      compact: "37",
      kind: "int",
    })
  })

  it("compacts committed references without losing the full ID", () => {
    const commit =
      "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
    expect(displayRowRef({ existing: { commit, counter: 4 } })).toEqual({
      compact: "12345678…cdef:4",
      full: `${commit}:4`,
      kind: "ref",
    })
  })
})
