// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Repo, isValidAutomergeUrl } from "@automerge/automerge-repo"
import { create, find } from "@coln-project/repo"
// @ts-expect-error initSync is exported at runtime but absent from declarations
import { initSync } from "@automerge/automerge-subduction/slim"
// @ts-expect-error wasm-base64 is exported without declarations
import { wasmBase64 } from "@automerge/automerge-subduction/wasm-base64"
import { mount } from "svelte"
import * as GraphRealm from "../generated/GraphRealm.ts"
import App from "./App.svelte"
import DocumentLoadError from "./DocumentLoadError.svelte"
import "./style.css"

initSync({ module: Uint8Array.from(atob(wasmBase64), char => char.charCodeAt(0)) })

const endpoint = import.meta.env.VITE_SUBDUCTION_ENDPOINT
  ?? "wss://subduction.sync.inkandswitch.com"
const hashUrl = location.hash.slice(1)
const documentUrl = isValidAutomergeUrl(hashUrl) ? hashUrl : undefined
const target = document.getElementById("app")!

if (hashUrl && !documentUrl) {
  mount(DocumentLoadError, {
    target,
    props: { documentUrl: hashUrl, kind: "invalid" },
  })
} else {
  const repo = new Repo({ subductionWebsocketEndpoints: [endpoint] })

  try {
    const handle = documentUrl
      ? await find(repo, documentUrl, GraphRealm)
      : create(repo, GraphRealm)

    location.hash = handle.url
    if (import.meta.env.DEV) Object.assign(window, { repo, handle })

    mount(App, {
      target,
      props: { handle, repo, endpoint },
    })
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
