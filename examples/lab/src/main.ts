// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Repo, type DocumentId } from "@automerge/automerge-repo"
import { IndexedDBStorageAdapter } from "@automerge/automerge-repo-storage-indexeddb"
// @ts-expect-error initSync is exported at runtime but absent from declarations
import { initSync } from "@automerge/automerge-subduction/slim"
// @ts-expect-error wasm-base64 is exported without declarations
import { wasmBase64 } from "@automerge/automerge-subduction/wasm-base64"
import { mount } from "svelte"
import LabApp from "./app/LabApp.svelte"
import "./style.css"

initSync({
  module: Uint8Array.from(atob(wasmBase64), (char) => char.charCodeAt(0)),
})

const endpoint =
  import.meta.env.VITE_SUBDUCTION_ENDPOINT ??
  "wss://coln.sync.inkandswitch.com"
const repo = new Repo({
  storage: new IndexedDBStorageAdapter("coln-lab", "documents"),
  subductionWebsocketEndpoints: [endpoint],
})
const openedDocuments = new Set<DocumentId>()
const trackDocument = (documentId: string) =>
  openedDocuments.add(documentId as DocumentId)
const flush = () => {
  if (openedDocuments.size > 0) void repo.flush([...openedDocuments])
}

addEventListener("pagehide", flush)
mount(LabApp, {
  target: document.getElementById("app")!,
  props: { repo, endpoint, trackDocument },
})

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    removeEventListener("pagehide", flush)
    void repo.shutdown()
  })
}
