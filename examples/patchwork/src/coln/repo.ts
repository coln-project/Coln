// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Repo, type DocumentId } from "@automerge/automerge-repo"
import { IndexedDBStorageAdapter } from "@automerge/automerge-repo-storage-indexeddb"

/**
 * Coln's own repo, living beside Patchwork's.
 *
 * A Coln store is not an automerge document: it is a Rust store whose commits
 * ride the Subduction sedimentree, registered with automerge-repo through
 * `defineDocumentType({ urlScheme: "coln" })`. That API only exists in the
 * automerge-repo fork this package bundles, and the stores sync through Coln's
 * relay rather than Patchwork's, so they cannot be served by `window.repo`.
 *
 * What is shared is the wasm underneath: automerge and Subduction come from
 * Patchwork's importmap (already initialised on the main thread by the host),
 * so this repo adds a websocket and an IndexedDB database, not another CRDT.
 */

const ENDPOINT_KEY = "coln-patchwork:sync-endpoint"
const DEFAULT_ENDPOINT = "wss://coln.sync.inkandswitch.com"

/**
 * The relay this page syncs Coln stores through. Override it per-browser with
 *
 *   localStorage["coln-patchwork:sync-endpoint"] = "ws://127.0.0.1:3030"
 *
 * which is how you point these tools at a local Subduction relay (`pnpm --dir
 * examples/lab server`). Read once, at first use: changing it takes a reload.
 */
export function syncEndpoint(): string {
  try {
    return localStorage.getItem(ENDPOINT_KEY) || DEFAULT_ENDPOINT
  } catch {
    return DEFAULT_ENDPOINT
  }
}

let repo: Repo | undefined
const openDocuments = new Set<DocumentId>()

export function colnRepo(): Repo {
  if (!repo) {
    repo = new Repo({
      storage: new IndexedDBStorageAdapter("coln-patchwork", "documents"),
      subductionWebsocketEndpoints: [syncEndpoint()],
    })
    addEventListener("pagehide", flush)
  }
  return repo
}

/**
 * Coln stores are flushed explicitly rather than on a timer, so every store a
 * tool opens is tracked and written out before the page goes away.
 */
export function trackDocument(documentId: DocumentId): void {
  openDocuments.add(documentId)
}

function flush(): void {
  if (repo && openDocuments.size > 0) void repo.flush([...openDocuments])
}
