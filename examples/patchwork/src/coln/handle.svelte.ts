// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type {
  ColnChange,
  ColnDocument,
  ColnHandle as RepoColnHandle,
  RealmBindings,
} from "@coln-project/repo"
import { createSubscriber } from "svelte/reactivity"

/**
 * A Coln store handle as reactive Svelte state. Coln documents change through
 * both `change` (a local transaction) and `heads-changed` (commits arriving
 * from the relay), so both are subscribed.
 */
export class ColnStore<Bindings extends RealmBindings | undefined = undefined> {
  readonly #subscribe: () => void

  constructor(private readonly handle: RepoColnHandle<Bindings>) {
    this.#subscribe = createSubscriber(update => {
      handle.on("change", update)
      handle.on("heads-changed", update)
      return () => {
        handle.off("change", update)
        handle.off("heads-changed", update)
      }
    })
  }

  get state(): ColnDocument<Bindings> {
    this.#subscribe()
    return this.handle.doc()
  }

  change(change: ColnChange<Bindings>): void {
    this.handle.change(change)
  }
}
