// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type {
  ColnDocument,
  ColnHandle as RepoColnHandle,
} from "@coln-project/repo"
import { createSubscriber } from "svelte/reactivity"

export class ColnHandleState {
  readonly #subscribe: () => void

  constructor(private readonly handle: RepoColnHandle) {
    this.#subscribe = createSubscriber((update) => {
      handle.on("change", update)
      return () => handle.off("change", update)
    })
  }

  get current(): ColnDocument {
    this.#subscribe()
    return this.handle.doc()
  }
}
