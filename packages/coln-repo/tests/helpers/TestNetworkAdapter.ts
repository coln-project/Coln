// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  NetworkAdapter,
  type Message,
  type PeerId,
} from "@automerge/automerge-repo"

export class TestNetworkAdapter extends NetworkAdapter {
  #connected = false
  #peer?: TestNetworkAdapter

  static createConnectedPair(): [TestNetworkAdapter, TestNetworkAdapter] {
    const first = new TestNetworkAdapter()
    const second = new TestNetworkAdapter()
    first.#peer = second
    second.#peer = first
    return [first, second]
  }

  isReady(): boolean {
    return true
  }

  async whenReady(): Promise<void> {}

  connect(peerId: PeerId): void {
    this.peerId = peerId
    this.#connected = true
  }

  disconnect(): void {
    this.#connected = false
  }

  announce(peerId: PeerId): void {
    this.emit("peer-candidate", { peerId, peerMetadata: {} })
  }

  send(message: Message): void {
    if (!this.#connected) return
    queueMicrotask(() => this.#peer?.receive(message))
  }

  private receive(message: Message): void {
    if (this.#connected) this.emit("message", message)
  }
}
