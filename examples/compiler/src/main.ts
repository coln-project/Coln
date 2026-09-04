// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  Repo,
  isValidAutomergeUrl,
  type AutomergeUrl,
} from "@automerge/automerge-repo"
import { IndexedDBStorageAdapter } from "@automerge/automerge-repo-storage-indexeddb"
// @ts-expect-error initSync is exported at runtime but absent from declarations
import { initSync } from "@automerge/automerge-subduction/slim"
// @ts-expect-error wasm-base64 is exported without declarations
import { wasmBase64 } from "@automerge/automerge-subduction/wasm-base64"
import { mount } from "svelte"
import App from "./browser/App.svelte"
import DocumentLoadError from "./browser/DocumentLoadError.svelte"
import {
  isTheoryDocument,
  newTheoryDocument,
  type TheoryDocument,
} from "./lib/theory-document.ts"
import "./style.css"

const target = document.getElementById("app")!
const hashUrl = location.hash.slice(1)

if (hashUrl && !isValidAutomergeUrl(hashUrl)) {
  mount(DocumentLoadError, {
    target,
    props: { documentUrl: hashUrl, kind: "invalid" },
  })
} else {
  initSync({ module: Uint8Array.from(atob(wasmBase64), char => char.charCodeAt(0)) })

  const endpoint = import.meta.env.VITE_SUBDUCTION_ENDPOINT
    ?? "wss://subduction.sync.inkandswitch.com"
  const repo = new Repo({
    storage: new IndexedDBStorageAdapter("coln-compiler", "documents"),
    subductionWebsocketEndpoints: [endpoint],
  })

  try {
    const handle = hashUrl
      ? await repo.find<TheoryDocument>(hashUrl as AutomergeUrl)
      : repo.create(newTheoryDocument())

    if (!isTheoryDocument(handle.doc())) {
      await repo.shutdown()
      mount(DocumentLoadError, {
        target,
        props: { documentUrl: hashUrl, kind: "incompatible" },
      })
    } else {
      if (!hashUrl) location.hash = handle.url
      const flush = () => void repo.flush([handle.documentId])
      addEventListener("pagehide", flush, { once: true })

      if (import.meta.env.DEV) Object.assign(window, { repo, handle })
      mount(App, {
        target,
        props: { repo, handle, endpoint },
      })
    }
  } catch (cause) {
    await repo.shutdown().catch(() => undefined)
    if (!isDocumentUnavailable(cause)) throw cause
    mount(DocumentLoadError, {
      target,
      props: { documentUrl: hashUrl, kind: "unavailable" },
    })
  }
}

function isDocumentUnavailable(cause: unknown): boolean {
  return cause instanceof Error && /^Document .+ is unavailable$/.test(cause.message)
}
