// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { patchworkRepo, type PatchworkMeta } from "../patchwork.ts"

/**
 * A Patchwork document that points at a Coln store.
 *
 * Patchwork addresses documents by `automerge:` url and opens them through its
 * own repo, so a `coln:` store can never be a Patchwork document itself. This
 * one-field document is the bridge: Patchwork owns the identity — title,
 * sidebar entry, links, sharing — while the rows live in the Coln repo at
 * `storeUrl`. Every Coln tool here edits one of these.
 */
export interface ColnPointerDoc {
  "@patchwork"?: PatchworkMeta
  title: string
  /** A `coln:` document url, or "" for a pointer that has yet to be aimed. */
  storeUrl: string
  /** Realm this store was created from, when known. Display only. */
  realmName?: string
  /** The theory this store was compiled from, for the trip back. */
  theoryUrl?: string
}

/** Create a Patchwork pointer document for an existing Coln store. */
export async function createPointerDoc(pointer: {
  type: string
  title: string
  storeUrl: string
  realmName?: string
  theoryUrl?: string
}): Promise<string> {
  const { type, title, storeUrl, realmName, theoryUrl } = pointer
  const handle = await patchworkRepo().create2<ColnPointerDoc>({
    "@patchwork": { type },
    title,
    storeUrl,
    // Spread rather than assigned: automerge has no undefined to store.
    ...(realmName ? { realmName } : {}),
    ...(theoryUrl ? { theoryUrl } : {}),
  })
  return handle.url
}

/**
 * The datatype half of a Coln tool. Both Coln datatypes are the same document
 * with a different id, so they share this.
 */
export function pointerDatatype(type: string, fallbackTitle: string) {
  return {
    init(doc: ColnPointerDoc) {
      doc["@patchwork"] = { type }
      doc.title = fallbackTitle
      doc.storeUrl = ""
    },
    getTitle(doc: ColnPointerDoc) {
      return doc.title || fallbackTitle
    },
    setTitle(doc: ColnPointerDoc, title: string) {
      doc.title = title
    },
    markCopy(doc: ColnPointerDoc) {
      // A copy points at the same store: Coln stores are shared by url, and
      // copying the pointer does not fork the store behind it.
      doc.title = `Copy of ${doc.title || fallbackTitle}`
    },
  }
}
