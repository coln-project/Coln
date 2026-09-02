// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { WebSocketServer } from "ws"
import {
  MemorySigner,
  MemoryStorage,
  Subduction,
} from "@automerge/automerge-subduction"

class WsTransport {
  #ws
  #queue = []
  #waiters = []
  #errorWaiters = []
  #closed = false
  #closedPromise
  #resolveClosed
  #disconnectCallback = null

  constructor(ws) {
    this.#ws = ws
    this.#closedPromise = new Promise(resolve => {
      this.#resolveClosed = resolve
    })

    ws.on("message", data => {
      const bytes = toUint8Array(data)
      const waiter = this.#waiters.shift()
      if (waiter) {
        this.#errorWaiters.shift()
        waiter(bytes)
      } else {
        this.#queue.push(bytes)
      }
    })

    ws.on("close", () => this.#markClosed(new Error("WebSocket closed")))
    ws.on("error", err => this.#markClosed(err instanceof Error ? err : new Error(String(err))))
  }

  onDisconnect(callback) {
    this.#disconnectCallback = callback
  }

  async sendBytes(bytes) {
    if (this.#closed) throw new Error("WebSocket closed")
    await new Promise((resolve, reject) => {
      this.#ws.send(Buffer.from(bytes), err => (err ? reject(err) : resolve()))
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
    this.#ws.close()
  }

  closed() {
    return this.#closedPromise
  }

  #markClosed(err, notify = true) {
    if (this.#closed) return
    this.#closed = true
    this.#resolveClosed()
    for (const reject of this.#errorWaiters) reject(err)
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

const port = Number(process.env.PORT ?? 3030)
const host = "127.0.0.1"
const serviceName = process.env.SUBDUCTION_SERVICE_NAME ?? `${host}:${port}`
const subduction = new Subduction({
  signer: new MemorySigner(),
  storage: new MemoryStorage(),
})
const wss = new WebSocketServer({ host, port })
const leases = new Set()
const clients = new Set()
let managed = process.env.COLN_SYNC_MANAGED === "1"
let shutdownTimer
let shuttingDown = false

wss.on("connection", (ws, request) => {
  if (shuttingDown) {
    ws.close()
    return
  }
  if (request.url === "/.coln-sync/lease") {
    clearTimeout(shutdownTimer)
    managed = true
    leases.add(ws)
    ws.send(JSON.stringify({ protocol: "coln-sync-relay", pid: process.pid }))
    ws.once("close", () => {
      leases.delete(ws)
      scheduleShutdown()
    })
    return
  }

  const transport = new WsTransport(ws)
  clients.add(ws)
  ws.once("close", () => clients.delete(ws))
  subduction.acceptTransport(transport, serviceName).catch(err => {
    if (!String(err?.message ?? err).includes("closed")) console.error(err)
  })
})

function scheduleShutdown() {
  if (!managed || leases.size > 0 || shuttingDown) return
  clearTimeout(shutdownTimer)
  shutdownTimer = setTimeout(shutdown, 500)
}

async function shutdown() {
  if (shuttingDown) return
  shuttingDown = true
  clearTimeout(shutdownTimer)
  const closed = new Promise(resolve => wss.close(resolve))
  for (const lease of leases) lease.close()
  for (const client of clients) client.close()
  await subduction.disconnectAll().catch(() => {})
  await closed
  process.exit(0)
}

process.on("SIGINT", shutdown)
process.on("SIGTERM", shutdown)

wss.on("listening", () => {
  console.log(`Subduction relay listening on ws://${host}:${port}`)
  if (managed) shutdownTimer = setTimeout(shutdown, 10_000)
})

wss.on("error", error => {
  if (error.code !== "EADDRINUSE") console.error(error)
  process.exit(1)
})
