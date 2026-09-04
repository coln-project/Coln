// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { createSubscriber } from "svelte/reactivity"
import type { TheoryDocument, TheoryDocumentHandle } from "./theory-document.ts"

export class TheoryHandle {
  readonly #subscribe: () => void

  constructor(private readonly handle: TheoryDocumentHandle) {
    this.#subscribe = createSubscriber(update => {
      handle.on("change", update)
      return () => handle.off("change", update)
    })
  }

  get state(): TheoryDocument {
    this.#subscribe()
    return this.handle.doc()
  }
}
