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
const serviceName = `localhost:${port}`
const subduction = new Subduction({
  signer: new MemorySigner(),
  storage: new MemoryStorage(),
})
const wss = new WebSocketServer({ port })

wss.on("connection", ws => {
  subduction.acceptTransport(new WsTransport(ws), serviceName).catch(err => {
    if (!String(err?.message ?? err).includes("closed")) console.error(err)
  })
})

process.on("SIGINT", async () => {
  await subduction.disconnectAll().catch(() => {})
  wss.close(() => process.exit(0))
})

console.log(`Subduction relay listening on ws://localhost:${port}`)
