// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { createSubscriber } from "svelte/reactivity"
import type {
  StoreRecord,
  TheoryDocument,
  TheoryDocumentHandle,
} from "./theory-document.ts"

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

  get documentId(): TheoryDocumentHandle["documentId"] {
    return this.handle.documentId
  }

  get heads(): string[] {
    return [...this.handle.heads()]
  }

  appendStore(record: StoreRecord): void {
    this.handle.change(document => {
      document.stores.push(record)
    })
  }
}
