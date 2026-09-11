// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { DocumentId } from "@automerge/automerge-repo"
import { createSubscriber } from "svelte/reactivity"
import { colnRepo } from "./repo.ts"

type FlushableHandle = {
  documentId: DocumentId
  on(event: "change", listener: () => void): void
  off(event: "change", listener: () => void): void
}

export type SyncStatus = "offline" | "syncing" | "synced" | "error"

/**
 * Sync state for one Coln store. Patchwork's own sync indicator watches the
 * host repo, which knows nothing about Coln's relay, so Coln tools report
 * their own: a store is "synced" once a flush to the relay has completed with
 * no newer local change waiting behind it.
 */
export class StoreSync {
  readonly #subscribe: () => void
  #status: SyncStatus
  #error: unknown

  constructor(private readonly handle: FlushableHandle) {
    const repo = colnRepo()
    this.#status = repo.isSubductionConnected() ? "syncing" : "offline"
    this.#subscribe = createSubscriber(update => {
      let active = true
      let generation = 0
      let currentAttempt: symbol | undefined

      const setStatus = (status: SyncStatus, error?: unknown) => {
        if (this.#status === status && this.#error === error) return
        this.#status = status
        this.#error = error
        update()
      }
      const finishFlush = (
        attempt: symbol,
        flushGeneration: number,
        failed: boolean,
        error?: unknown,
      ) => {
        if (!active || currentAttempt !== attempt) return
        currentAttempt = undefined
        if (!repo.isSubductionConnected()) setStatus("offline")
        else if (flushGeneration !== generation) startFlush()
        else if (failed) setStatus("error", error)
        else setStatus("synced")
      }
      const startFlush = () => {
        if (currentAttempt || !active || !repo.isSubductionConnected()) return
        const attempt = Symbol()
        const flushGeneration = generation
        currentAttempt = attempt
        void repo.flush([this.handle.documentId]).then(
          () => finishFlush(attempt, flushGeneration, false),
          (error: unknown) => finishFlush(attempt, flushGeneration, true, error),
        )
      }
      const requestFlush = () => {
        generation += 1
        if (!repo.isSubductionConnected()) {
          currentAttempt = undefined
          setStatus("offline")
          return
        }
        setStatus("syncing")
        startFlush()
      }

      this.handle.on("change", requestFlush)
      repo.on("subduction-connection", requestFlush)
      requestFlush()
      return () => {
        active = false
        generation += 1
        currentAttempt = undefined
        this.handle.off("change", requestFlush)
        repo.off("subduction-connection", requestFlush)
      }
    })
  }

  get status(): SyncStatus {
    this.#subscribe()
    return this.#status
  }

  get error(): unknown {
    this.#subscribe()
    return this.#error
  }

  get message(): string {
    const error = this.error
    if (error === undefined) return ""
    return error instanceof Error ? error.message : String(error)
  }
}
