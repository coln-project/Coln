// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { TheoryDocument } from "./theory-document.ts"

export const THEORY_TYPE = "coln-theory"

/**
 * An ordinary Patchwork document: `source` is collaborative text, `stores`
 * records the Coln stores compiled out of it. Nothing here needs Coln's repo,
 * so a theory syncs, forks, versions and comments like any other Patchwork
 * document — only the stores it links to live in Coln's world.
 */
export const ColnTheoryDatatype = {
  init(doc: TheoryDocument) {
    doc["@patchwork"] = { type: THEORY_TYPE }
    doc.version = 1
    doc.title = "Untitled theory"
    doc.source = ""
    doc.stores = []
  },

  getTitle(doc: TheoryDocument) {
    return doc.title || "Untitled theory"
  },

  setTitle(doc: TheoryDocument, title: string) {
    doc.title = title
  },

  markCopy(doc: TheoryDocument) {
    doc.title = `Copy of ${doc.title || "Untitled theory"}`
  },
}

export default ColnTheoryDatatype
