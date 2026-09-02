// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { spawn } from "node:child_process"
import { fileURLToPath } from "node:url"
import WebSocket from "ws"

const [, , command, ...args] = process.argv

if (!command) {
  console.error("Usage: node server/with-server.mjs <command> [args...]")
  process.exit(1)
}

const host = "127.0.0.1"
const port = Number(process.env.SUBDUCTION_PORT ?? 3030)
const endpoint = `ws://${host}:${port}`
const leaseUrl = `${endpoint}/.coln-sync/lease`
const serverPath = fileURLToPath(new URL("server.mjs", import.meta.url))

let lease = await acquireLease()
if (!lease) {
  const server = spawn(process.execPath, [serverPath], {
    detached: true,
    env: {
      ...process.env,
      COLN_SYNC_MANAGED: "1",
      PORT: String(port),
      SUBDUCTION_SERVICE_NAME: `${host}:${port}`,
    },
    stdio: "ignore",
  })
  server.unref()
  lease = await waitForLease()
}

if (!lease) {
  console.error(`Could not start the local Subduction relay at ${endpoint}`)
  process.exit(1)
}

const child = spawn(command, args, {
  env: {
    ...process.env,
    SUBDUCTION_ENDPOINT: endpoint,
    VITE_SUBDUCTION_ENDPOINT: endpoint,
  },
  stdio: "inherit",
})

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.once(signal, () => child.kill(signal))
}

const exitCode = await new Promise(resolve => {
  child.once("error", error => {
    console.error(error.message)
    resolve(1)
  })
  child.once("exit", (code, signal) => {
    resolve(code ?? (signal === "SIGINT" ? 130 : 143))
  })
})

lease.close()
await Promise.race([
  new Promise(resolve => lease.once("close", resolve)),
  new Promise(resolve => setTimeout(resolve, 250)),
])
process.exitCode = exitCode

async function waitForLease() {
  const deadline = Date.now() + 10_000
  while (Date.now() < deadline) {
    const lease = await acquireLease()
    if (lease) return lease
    await new Promise(resolve => setTimeout(resolve, 50))
  }
  return null
}

function acquireLease() {
  return new Promise(resolve => {
    const socket = new WebSocket(leaseUrl)
    const timeout = setTimeout(() => finish(null), 500)
    let settled = false

    const finish = result => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      if (!result) socket.close()
      resolve(result)
    }

    socket.once("message", data => {
      try {
        const message = JSON.parse(String(data))
        finish(message.protocol === "coln-sync-relay" ? socket : null)
      } catch {
        finish(null)
      }
    })
    socket.once("error", () => finish(null))
    socket.once("close", () => finish(null))
  })
}
