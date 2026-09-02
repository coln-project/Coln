// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  Repo,
  initSubduction,
  isValidAutomergeUrl,
  type AutomergeUrl,
} from "@automerge/automerge-repo"
import { create, find } from "@coln-project/repo"
import { render } from "ink"
import * as GraphRealm from "../generated/GraphRealm.ts"
import App from "./App.tsx"

const endpoint = process.env.SUBDUCTION_ENDPOINT
  ?? "wss://subduction.sync.inkandswitch.com"
const url = process.argv[2]

if (url && !isValidAutomergeUrl(url)) {
  console.error("Expected an automerge URL or no argument to create a document.")
  process.exit(1)
}
if (!process.stdin.isTTY || !process.stdout.isTTY) {
  console.error("The sync CLI requires an interactive terminal.")
  process.exit(1)
}

await initSubduction()
const repo = new Repo({
  subductionWebsocketEndpoints: [endpoint],
  subductionTimeouts: { syncMs: 5_000, defaultMs: 5_000 },
})
await waitForSubduction(repo, endpoint)
const handle = url
  ? await find(repo, url as AutomergeUrl, GraphRealm)
  : create(repo, GraphRealm)

let requestQuit!: () => void
const quitRequested = new Promise<void>(resolve => {
  requestQuit = resolve
})

let instance: ReturnType<typeof render> | undefined
let alternateScreenActive = true
const signalHandlers = new Map<NodeJS.Signals, () => void>()

process.stdout.write("\u001B[?1049h")
for (const signal of ["SIGHUP", "SIGINT", "SIGTERM"] as const) {
  const handler = () => {
    removeSignalHandlers()
    instance?.unmount()
    leaveAlternateScreen()
    process.kill(process.pid, signal)
  }
  signalHandlers.set(signal, handler)
  process.once(signal, handler)
}

try {
  instance = render(
    <App handle={handle} repo={repo} endpoint={endpoint} onQuit={requestQuit} />,
    { exitOnCtrlC: false },
  )
  try {
    await Promise.race([quitRequested, instance.waitUntilExit()])
  } finally {
    await repo.shutdown()
  }
} finally {
  removeSignalHandlers()
  instance?.unmount()
  leaveAlternateScreen()
}

function removeSignalHandlers(): void {
  for (const [signal, handler] of signalHandlers) {
    process.off(signal, handler)
  }
  signalHandlers.clear()
}

function leaveAlternateScreen(): void {
  if (!alternateScreenActive) return
  alternateScreenActive = false
  process.stdout.write("\u001B[?1049l")
}

async function waitForSubduction(repo: Repo, endpoint: string): Promise<void> {
  if (repo.isSubductionConnected()) return

  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      cleanup()
      reject(new Error(`Timed out connecting to ${endpoint}`))
    }, 10_000)
    const connected = ({ connected }: { connected: boolean }) => {
      if (!connected) return
      cleanup()
      resolve()
    }
    const cleanup = () => {
      clearTimeout(timeout)
      repo.off("subduction-connection", connected)
    }

    repo.on("subduction-connection", connected)
    if (repo.isSubductionConnected()) {
      cleanup()
      resolve()
    }
  })
}
