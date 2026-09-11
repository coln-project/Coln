// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { describe, expect, it } from "vitest"
import { pointerDatatype, type ColnPointerDoc } from "../src/coln/pointer.ts"
import { ColnTheoryDatatype } from "../src/theory/datatype.ts"
import {
  isTheoryDocument,
  type TheoryDocument,
} from "../src/theory/theory-document.ts"

describe("theory datatype", () => {
  it("seeds a document Patchwork can title and Coln can compile", () => {
    const doc = {} as TheoryDocument
    ColnTheoryDatatype.init(doc)

    expect(doc["@patchwork"]).toEqual({ type: "coln-theory" })
    expect(ColnTheoryDatatype.getTitle(doc)).toBe("Untitled theory")
    expect(isTheoryDocument(doc)).toBe(true)
  })

  it("renames and copies through the title field", () => {
    const doc = {} as TheoryDocument
    ColnTheoryDatatype.init(doc)
    ColnTheoryDatatype.setTitle(doc, "Graphs")
    expect(ColnTheoryDatatype.getTitle(doc)).toBe("Graphs")
    ColnTheoryDatatype.markCopy(doc)
    expect(ColnTheoryDatatype.getTitle(doc)).toBe("Copy of Graphs")
  })

  it("rejects documents that are not theories", () => {
    expect(isTheoryDocument({ version: 1, source: "", stores: [{}] })).toBe(false)
    expect(isTheoryDocument({ version: 2, source: "", stores: [] })).toBe(false)
    expect(isTheoryDocument(null)).toBe(false)
  })

  it("accepts a theory written by Coln Lab, which has no title", () => {
    const lab = {
      version: 1,
      source: "theory Graph := sig end",
      stores: [
        {
          url: "coln:abc",
          createdAt: 1,
          sourceHeads: ["head"],
          realmIndex: 0,
          realmName: "GraphRealm",
          ir: { entities: [], rules: [] },
        },
      ],
    }
    expect(isTheoryDocument(lab)).toBe(true)
  })
})

describe("pointer datatype", () => {
  const datatype = pointerDatatype("coln-store", "Coln store")

  it("starts unaimed, so the tool asks for a store url", () => {
    const doc = {} as ColnPointerDoc
    datatype.init(doc)
    expect(doc).toEqual({
      "@patchwork": { type: "coln-store" },
      title: "Coln store",
      storeUrl: "",
    })
  })

  it("keeps no undefined fields, which automerge cannot store", () => {
    const doc = {} as ColnPointerDoc
    datatype.init(doc)
    expect(Object.values(doc).every(value => value !== undefined)).toBe(true)
  })
})
