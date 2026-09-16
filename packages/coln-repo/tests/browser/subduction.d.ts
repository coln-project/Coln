// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

declare module "@automerge/automerge-subduction/slim" {
  export * from "@automerge/automerge-subduction"
  export function initSync(input: { module: Uint8Array }): void
}

declare module "@automerge/automerge-subduction/wasm-base64" {
  export const wasmBase64: string
}
