// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { createSubscriber } from "svelte/reactivity"
import type { PatchworkHandle } from "../patchwork.ts"

/** A Patchwork document as reactive Svelte state. */
export class DocState<Doc> {
  readonly #subscribe: () => void

  constructor(private readonly handle: PatchworkHandle<Doc>) {
    this.#subscribe = createSubscriber(update => {
      handle.on("change", update)
      return () => handle.off("change", update)
    })
  }

  get current(): Doc {
    this.#subscribe()
    return this.handle.doc()
  }
}
