// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { Repo } from "@automerge/automerge-repo"
import type { ColnHandle } from "@coln-project/repo"
import { createSubscriber } from "svelte/reactivity"

export type SyncStatus = "offline" | "syncing" | "synced" | "error"

export class SubductionSync {
  readonly #subscribe: () => void
  #status: SyncStatus
  #error: unknown

  constructor(
    private readonly repo: Repo,
    private readonly handle: ColnHandle,
  ) {
    this.#status = repo.isSubductionConnected() ? "syncing" : "offline"
    this.#subscribe = createSubscriber((update) => {
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
        if (!this.repo.isSubductionConnected()) setStatus("offline")
        else if (flushGeneration !== generation) startFlush()
        else if (failed) setStatus("error", error)
        else setStatus("synced")
      }
      const startFlush = () => {
        if (currentAttempt || !active || !this.repo.isSubductionConnected())
          return
        const attempt = Symbol()
        const flushGeneration = generation
        currentAttempt = attempt
        void this.repo.flush([this.handle.documentId]).then(
          () => finishFlush(attempt, flushGeneration, false),
          (error) => finishFlush(attempt, flushGeneration, true, error),
        )
      }
      const requestFlush = () => {
        generation += 1
        if (!this.repo.isSubductionConnected()) {
          currentAttempt = undefined
          setStatus("offline")
          return
        }
        setStatus("syncing")
        startFlush()
      }

      this.handle.on("change", requestFlush)
      this.repo.on("subduction-connection", requestFlush)
      requestFlush()
      return () => {
        active = false
        generation += 1
        currentAttempt = undefined
        this.handle.off("change", requestFlush)
        this.repo.off("subduction-connection", requestFlush)
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
}
