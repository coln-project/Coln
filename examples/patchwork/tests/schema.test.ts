// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { describe, expect, it } from "vitest"
import { readTables } from "../src/store/schema.ts"

describe("readTables", () => {
  it("extracts paths, types, and primary columns", () => {
    const tables = readTables(
      JSON.stringify({
        entities: [
          {
            path: [["Realm"], ["profile"]],
            value: {
              entityVariant: { tag: "table" },
              columns: [
                {
                  path: [["owner"]],
                  type: { tag: "rowId", path: [["Realm"], ["Person"]] },
                },
                {
                  path: [["details"], ["name"]],
                  type: { tag: "builtin", type: "builtinString" },
                },
              ],
              primaryKey: [[["owner"]]],
            },
          },
        ],
        rules: [],
      }),
    )

    expect(tables).toEqual([
      {
        name: "Realm.profile",
        columns: [
          {
            name: "owner",
            type: "ref Realm.Person",
            primary: true,
            referenceTable: "Realm.Person",
          },
          { name: "details.name", type: "string", primary: false },
        ],
      },
    ])
  })

  it("rejects malformed IR", () => {
    expect(() => readTables('{"rules":[]}')).toThrow("entities array")
    expect(() =>
      readTables(
        JSON.stringify({
          entities: [
            { path: [["Realm"]], value: { entityVariant: { tag: "view" } } },
          ],
        }),
      ),
    ).toThrow("is not a table")
  })
})
