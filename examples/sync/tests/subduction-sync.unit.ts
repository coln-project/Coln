// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { Repo } from "@automerge/automerge-repo"
import type { ColnHandle } from "@coln-project/repo"
import { flushSync } from "svelte"
import { describe, expect, test } from "vitest"
import * as GraphRealm from "../src/generated/GraphRealm.ts"
import { SubductionSync, type SubductionSyncStatus } from "../src/lib/subduction-sync.svelte.ts"
import { observe } from "./observe.svelte.ts"

type GraphHandle = ColnHandle<typeof GraphRealm>

describe("SubductionSync", () => {
  test("moves from offline through syncing to synced and cleans up", async () => {
    const repo = new FakeRepo(false)
    const handle = new FakeHandle()
    const sync = new SubductionSync(repo as unknown as Repo, handle as unknown as GraphHandle)
    let status: SubductionSyncStatus | undefined
    const stop = observe(() => sync.status, value => {
      status = value
    })
    flushSync()

    expect(status).toBe("offline")
    expect(repo.listenerCount()).toBe(1)
    expect(handle.listenerCount()).toBe(1)
    expect(repo.flushes).toHaveLength(0)

    repo.connect()
    flushSync()
    expect(status).toBe("syncing")
    expect(repo.flushes).toHaveLength(1)
    expect(repo.flushedDocuments[0]).toEqual([handle.documentId])

    await repo.resolveFlush(0)
    flushSync()
    expect(status).toBe("synced")

    stop()
    await Promise.resolve()
    expect(repo.listenerCount()).toBe(0)
    expect(handle.listenerCount()).toBe(0)
  })

  test("does not report synced while a newer change is pending", async () => {
    const repo = new FakeRepo(true)
    const handle = new FakeHandle()
    const sync = new SubductionSync(repo as unknown as Repo, handle as unknown as GraphHandle)
    let status: SubductionSyncStatus | undefined
    const stop = observe(() => sync.status, value => {
      status = value
    })
    flushSync()

    await repo.resolveFlush(0)
    flushSync()
    expect(status).toBe("synced")

    handle.emitChange()
    flushSync()
    expect(status).toBe("syncing")
    expect(repo.flushes).toHaveLength(2)

    handle.emitChange()
    await repo.resolveFlush(1)
    flushSync()
    expect(status).toBe("syncing")
    expect(repo.flushes).toHaveLength(3)

    await repo.resolveFlush(2)
    flushSync()
    expect(status).toBe("synced")
    stop()
  })

  test("reports flush failures and retries after another change", async () => {
    const repo = new FakeRepo(true)
    const handle = new FakeHandle()
    const sync = new SubductionSync(repo as unknown as Repo, handle as unknown as GraphHandle)
    let observed: { status: SubductionSyncStatus; error: unknown } | undefined
    const stop = observe(() => ({ status: sync.status, error: sync.error }), value => {
      observed = value
    })
    flushSync()

    const failure = new Error("save failed")
    await repo.rejectFlush(0, failure)
    flushSync()
    expect(observed).toEqual({ status: "error", error: failure })

    handle.emitChange()
    flushSync()
    expect(observed?.status).toBe("syncing")
    await repo.resolveFlush(1)
    flushSync()
    expect(observed).toEqual({ status: "synced", error: undefined })
    stop()
  })

  test("reconnects without waiting for an obsolete flush", async () => {
    const repo = new FakeRepo(true)
    const handle = new FakeHandle()
    const sync = new SubductionSync(repo as unknown as Repo, handle as unknown as GraphHandle)
    let status: SubductionSyncStatus | undefined
    const stop = observe(() => sync.status, value => {
      status = value
    })
    flushSync()

    expect(repo.flushes).toHaveLength(1)
    repo.disconnect()
    flushSync()
    expect(status).toBe("offline")

    repo.connect()
    flushSync()
    expect(status).toBe("syncing")
    expect(repo.flushes).toHaveLength(2)

    await repo.resolveFlush(1)
    flushSync()
    expect(status).toBe("synced")

    await repo.rejectFlush(0, new Error("obsolete"))
    flushSync()
    expect(status).toBe("synced")
    stop()
  })

  test("ignores late completion after cleanup and can resubscribe", async () => {
    const repo = new FakeRepo(true)
    const handle = new FakeHandle()
    const sync = new SubductionSync(repo as unknown as Repo, handle as unknown as GraphHandle)
    let status: SubductionSyncStatus | undefined
    const firstStop = observe(() => sync.status, value => {
      status = value
    })
    flushSync()

    firstStop()
    await Promise.resolve()
    await repo.rejectFlush(0, new Error("late"))
    expect(repo.listenerCount()).toBe(0)
    expect(handle.listenerCount()).toBe(0)

    const secondStop = observe(() => sync.status, value => {
      status = value
    })
    flushSync()
    expect(repo.listenerCount()).toBe(1)
    expect(handle.listenerCount()).toBe(1)
    expect(repo.flushes).toHaveLength(2)
    expect(status).toBe("syncing")

    await repo.resolveFlush(1)
    flushSync()
    expect(status).toBe("synced")
    secondStop()
  })
})

class FakeHandle {
  readonly documentId = "test-document"
  readonly #listeners = new Set<() => void>()

  on(_event: "change", listener: () => void): void {
    this.#listeners.add(listener)
  }

  off(_event: "change", listener: () => void): void {
    this.#listeners.delete(listener)
  }

  emitChange(): void {
    for (const listener of this.#listeners) listener()
  }

  listenerCount(): number {
    return this.#listeners.size
  }
}

class FakeRepo {
  readonly flushes: Deferred<void>[] = []
  readonly flushedDocuments: unknown[] = []
  #connected: boolean
  readonly #listeners = new Set<() => void>()

  constructor(connected: boolean) {
    this.#connected = connected
  }

  isSubductionConnected(): boolean {
    return this.#connected
  }

  on(_event: "subduction-connection", listener: () => void): void {
    this.#listeners.add(listener)
  }

  off(_event: "subduction-connection", listener: () => void): void {
    this.#listeners.delete(listener)
  }

  flush(documents?: unknown): Promise<void> {
    const flush = deferred<void>()
    this.flushes.push(flush)
    this.flushedDocuments.push(documents)
    return flush.promise
  }

  connect(): void {
    this.#connected = true
    for (const listener of this.#listeners) listener()
  }

  disconnect(): void {
    this.#connected = false
    for (const listener of this.#listeners) listener()
  }

  async resolveFlush(index: number): Promise<void> {
    const flush = this.flushes[index]
    if (!flush) throw new Error(`Missing flush ${index}`)
    flush.resolve()
    await flush.promise
    await Promise.resolve()
  }

  async rejectFlush(index: number, error: unknown): Promise<void> {
    const flush = this.flushes[index]
    if (!flush) throw new Error(`Missing flush ${index}`)
    flush.reject(error)
    await flush.promise.catch(() => undefined)
    await Promise.resolve()
  }

  listenerCount(): number {
    return this.#listeners.size
  }
}

type Deferred<T> = {
  promise: Promise<T>
  resolve: (value: T | PromiseLike<T>) => void
  reject: (error: unknown) => void
}

function deferred<T>(): Deferred<T> {
  let resolve!: Deferred<T>["resolve"]
  let reject!: Deferred<T>["reject"]
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise
    reject = rejectPromise
  })
  return { promise, resolve, reject }
}
