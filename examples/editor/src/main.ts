// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// @ts-expect-error initSync is exported at runtime but absent from declarations
import { initSync } from "@automerge/automerge-subduction/slim"
// @ts-expect-error wasm-base64 is exported without declarations
import { wasmBase64 } from "@automerge/automerge-subduction/wasm-base64"
import { mount } from "svelte"
import App from "./browser/App.svelte"
import "./style.css"

initSync({
  module: Uint8Array.from(atob(wasmBase64), (char) => char.charCodeAt(0)),
})

const endpoint =
  import.meta.env.VITE_SUBDUCTION_ENDPOINT ??
  "wss://subduction.sync.inkandswitch.com"

mount(App, {
  target: document.getElementById("app")!,
  props: { endpoint },
})
