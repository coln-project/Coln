// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  initSubduction,
  isValidAutomergeUrl,
  Repo,
  type AutomergeUrl,
} from "@automerge/automerge-repo"
import { find, type ColnHandle } from "@coln-project/repo"

export const defaultEndpoint = "wss://subduction.sync.inkandswitch.com"
export const operationTimeoutMs = 10_000

export interface OpenDocument {
  repo: Repo
  handle: ColnHandle
}

export async function openDocument(
  documentUrl: string,
  endpoint: string,
  verbose: boolean,
): Promise<OpenDocument> {
  if (!isValidAutomergeUrl(documentUrl)) throw new Error(`Invalid Automerge URL: ${documentUrl}`)
  validateEndpoint(endpoint)
  if (verbose) console.error(`Connecting to ${endpoint}`)

  await initSubduction()
  const repo = new Repo({ subductionWebsocketEndpoints: [endpoint] })
  try {
    await withTimeout(waitForConnection(repo), `Timed out connecting to ${endpoint}`)
    if (verbose) console.error(`Connected to ${endpoint}`)
    const handle = await withTimeout(
      find(repo, documentUrl as AutomergeUrl),
      `Timed out loading ${documentUrl}`,
    )
    return { repo, handle }
  } catch (error) {
    await repo.shutdown()
    throw error
  }
}

export async function flush(repo: Repo, endpoint: string, verbose: boolean): Promise<void> {
  if (!repo.isSubductionConnected()) throw new Error(`Disconnected from ${endpoint} before sync`)
  await withTimeout(repo.flush(), `Timed out syncing to ${endpoint}`)
  if (verbose) console.error(`Synced to ${endpoint}`)
}

function waitForConnection(repo: Repo): Promise<void> {
  if (repo.isSubductionConnected()) return Promise.resolve()
  return new Promise(resolve => {
    const connected = ({ connected: isConnected }: { connected: boolean }) => {
      if (!isConnected) return
      repo.off("subduction-connection", connected)
      resolve()
    }
    repo.on("subduction-connection", connected)
    if (repo.isSubductionConnected()) {
      repo.off("subduction-connection", connected)
      resolve()
    }
  })
}

async function withTimeout<Value>(promise: Promise<Value>, message: string): Promise<Value> {
  let timeout: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_resolve, reject) => {
        timeout = setTimeout(() => reject(new Error(message)), operationTimeoutMs)
      }),
    ])
  } finally {
    if (timeout !== undefined) clearTimeout(timeout)
  }
}

function validateEndpoint(endpoint: string): void {
  let url: URL
  try {
    url = new URL(endpoint)
  } catch {
    throw new Error(`Invalid Subduction endpoint: ${endpoint}`)
  }
  if (url.protocol !== "ws:" && url.protocol !== "wss:") {
    throw new Error(`Subduction endpoint must use ws: or wss: ${endpoint}`)
  }
}
