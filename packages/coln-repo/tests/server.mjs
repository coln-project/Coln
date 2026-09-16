// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { WebSocketServer } from "ws"
import {
  MemorySigner,
  MemoryStorage,
  Subduction,
} from "@automerge/automerge-subduction"

class WebSocketTransport {
  #socket
  #queue = []
  #waiters = []
  #errorWaiters = []
  #closed = false
  #closedPromise
  #resolveClosed
  #disconnectCallback = null

  constructor(socket) {
    this.#socket = socket
    this.#closedPromise = new Promise(resolve => {
      this.#resolveClosed = resolve
    })

    socket.on("message", data => {
      const bytes = toUint8Array(data)
      const waiter = this.#waiters.shift()
      if (waiter) {
        this.#errorWaiters.shift()
        waiter(bytes)
      } else {
        this.#queue.push(bytes)
      }
    })
    socket.on("close", () => this.#markClosed(new Error("WebSocket closed")))
    socket.on("error", error => this.#markClosed(error))
  }

  onDisconnect(callback) {
    this.#disconnectCallback = callback
  }

  async sendBytes(bytes) {
    if (this.#closed) throw new Error("WebSocket closed")
    await new Promise((resolve, reject) => {
      this.#socket.send(Buffer.from(bytes), error => (error ? reject(error) : resolve()))
    })
  }

  recvBytes() {
    const queued = this.#queue.shift()
    if (queued) return Promise.resolve(queued)
    if (this.#closed) return Promise.reject(new Error("WebSocket closed"))
    return new Promise((resolve, reject) => {
      this.#waiters.push(resolve)
      this.#errorWaiters.push(reject)
    })
  }

  async disconnect() {
    this.#markClosed(new Error("WebSocket closed"), false)
    this.#socket.close()
  }

  closed() {
    return this.#closedPromise
  }

  #markClosed(error, notify = true) {
    if (this.#closed) return
    this.#closed = true
    this.#resolveClosed()
    for (const reject of this.#errorWaiters) reject(error)
    this.#waiters = []
    this.#errorWaiters = []
    if (notify) this.#disconnectCallback?.()
  }
}

function toUint8Array(data) {
  if (data instanceof Uint8Array) {
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
  }
  if (data instanceof ArrayBuffer) return new Uint8Array(data)
  if (Array.isArray(data)) return toUint8Array(Buffer.concat(data))
  return new Uint8Array(Buffer.from(data))
}

const port = 3031
const serviceName = `127.0.0.1:${port}`
const subduction = new Subduction({
  signer: new MemorySigner(),
  storage: new MemoryStorage(),
})
const server = new WebSocketServer({ port })

server.on("connection", socket => {
  subduction.acceptTransport(new WebSocketTransport(socket), serviceName).catch(error => {
    if (!String(error?.message ?? error).includes("closed")) console.error(error)
  })
})

async function shutdown() {
  await subduction.disconnectAll().catch(() => {})
  server.close(() => process.exit(0))
}

process.on("SIGINT", shutdown)
process.on("SIGTERM", shutdown)

console.log(`Subduction test relay listening on ws://127.0.0.1:${port}`)
