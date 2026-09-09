// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  initSubduction,
  Repo,
} from "@automerge/automerge-repo"
import { setTimeout as delay } from "node:timers/promises"
import { TestNetworkAdapter } from "./TestNetworkAdapter"

export interface TestRepoPair {
  source: Repo
  replica: Repo
  shutdown(): Promise<void>
}

export async function createTestRepoPair(): Promise<TestRepoPair> {
  await initSubduction()
  const [sourceAdapter, replicaAdapter] = TestNetworkAdapter.createConnectedPair()
  const source = new Repo({
    subductionAdapters: [{ adapter: sourceAdapter, serviceName: "test", role: "connect" }],
    subductionTimeouts: { healMaxAttempts: 0, syncMs: 1_000 },
  })
  const replica = new Repo({
    subductionAdapters: [
      { adapter: replicaAdapter, serviceName: "test", role: "accept" },
    ],
    subductionTimeouts: { healMaxAttempts: 0, syncMs: 1_000 },
  })

  const sourceConnected = waitForConnection(source)
  const replicaConnected = waitForConnection(replica)
  sourceAdapter.announce(replica.peerId)
  replicaAdapter.announce(source.peerId)
  await Promise.all([sourceConnected, replicaConnected])

  return {
    source,
    replica,
    async shutdown() {
      await Promise.all([source.flush(), replica.flush()])
      await Promise.all([
        source.subduction.then(subduction => subduction.disconnectAll()),
        replica.subduction.then(subduction => subduction.disconnectAll()),
      ])
      await Promise.all([source.shutdown(), replica.shutdown()])
    },
  }
}

export function waitForChange(handle: {
  once(event: "change", listener: () => void): unknown
}): Promise<void> {
  return withTimeout(
    new Promise(resolve => handle.once("change", () => resolve())),
    "handle change",
  )
}

function waitForConnection(repo: Repo): Promise<void> {
  if (repo.isSubductionConnected()) return Promise.resolve()
  return withTimeout(
    new Promise(resolve => {
      const onConnection = ({ connected }: { connected: boolean }) => {
        if (!connected) return
        repo.off("subduction-connection", onConnection)
        resolve()
      }
      repo.on("subduction-connection", onConnection)
    }),
    "Subduction connection",
  )
}

function withTimeout(promise: Promise<void>, operation: string): Promise<void> {
  return Promise.race([
    promise,
    delay(5_000, undefined, { ref: false }).then(() => {
      throw new Error(`${operation} timed out`)
    }),
  ])
}
