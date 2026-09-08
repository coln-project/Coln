// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/**
 * The slice of Patchwork this package uses.
 *
 * Deliberately hand-written rather than imported from automerge-repo: this
 * bundle resolves `@automerge/automerge-repo` to Coln's fork (see
 * vite.config.ts), and Patchwork's documents belong to the host's copy. Typing
 * the host side structurally keeps the two worlds from being confused for one
 * another — a Patchwork handle never reaches Coln's `Repo`, and a `coln:` store
 * handle never reaches Patchwork's.
 */

export interface PatchworkHandle<Doc> {
  readonly url: string
  readonly documentId: string
  doc(): Doc
  heads(): string[]
  change(mutate: (doc: Doc) => void): void
  on(event: "change", listener: () => void): void
  off(event: "change", listener: () => void): void
}

export interface PatchworkRepo {
  create2<Doc>(initial: Doc): Promise<PatchworkHandle<Doc>>
  find<Doc>(url: string): Promise<PatchworkHandle<Doc>>
}

/** Metadata Patchwork reads to decide which datatype a document is. */
export type PatchworkMeta = { type: string; title?: string }

/** A Patchwork tool: mount into `element`, return a teardown function. */
export type PatchworkTool<Doc> = (
  handle: PatchworkHandle<Doc>,
  element: HTMLElement,
) => () => void

export function patchworkRepo(): PatchworkRepo {
  const repo = (globalThis as { repo?: PatchworkRepo }).repo
  if (!repo) throw new Error("Patchwork's repo is not available on window.repo")
  return repo
}

/**
 * Navigate Patchwork to another document. Dispatched rather than imported from
 * `@inkandswitch/patchwork-elements` so a host without that package still
 * loads these tools; the event is inert if nothing listens.
 */
export function openDocument(
  element: HTMLElement,
  url: string,
  toolId?: string,
): void {
  element.dispatchEvent(
    new CustomEvent("patchwork:open-document", {
      bubbles: true,
      composed: true,
      detail: { url, toolId },
    }),
  )
}
