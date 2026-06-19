import React from "react"
import ReactDOM from "react-dom/client"
import {
  Repo,
  isValidAutomergeUrl,
  type CrdtDocHandle,
} from "@automerge/automerge-repo"
// @ts-ignore initSync is exported at runtime but may be missing from declarations
import { initSync } from "@automerge/automerge-subduction/slim"
// @ts-ignore wasm-base64 may not have declarations
import { wasmBase64 } from "@automerge/automerge-subduction/wasm-base64"
import { App } from "./App"
import { colnDocType, type ColnDocType } from "./colnDocType"
import * as GraphRealm from "./generated/GraphRealm.ts"

initSync({ module: Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0)) })

type GraphDocType = ColnDocType<typeof GraphRealm>

const coln = colnDocType(GraphRealm)
const repo = new Repo({
  subductionWebsocketEndpoints: ["wss://subduction.sync.inkandswitch.com"],
})

const hashUrl = location.hash.slice(1)
let handle: CrdtDocHandle<GraphDocType>

if (isValidAutomergeUrl(hashUrl)) {
  handle = await repo.find(hashUrl, coln)
} else {
  handle = repo.create(undefined, coln)
}

location.hash = handle.url

Object.assign(window, { repo, handle })

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App handle={handle} />
  </React.StrictMode>
)
