// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  Repo,
  type AutomergeUrl,
  type CrdtDocHandle,
} from "@automerge/automerge-repo"
import { initSync } from "@automerge/automerge-subduction/slim"
import { wasmBase64 } from "@automerge/automerge-subduction/wasm-base64"
import type { RowView, Value } from "@coln-project/runtime"
import {
  colnDocType,
  applyBindings,
  type ColnHandle,
  type ColnSchema,
} from "../../src/index.js"
import * as itemFfi from "../fixtures/itemFfi"

initSync({ module: Uint8Array.from(atob(wasmBase64), char => char.charCodeAt(0)) })

type RawHandle = CrdtDocHandle<typeof colnDocType>
type ItemHandle = ColnHandle<typeof itemFfi>

const repo = new Repo({
  subductionWebsocketEndpoints: ["ws://127.0.0.1:3031"],
})
let handle: RawHandle | undefined
let typedHandle: ItemHandle | undefined

const api = {
  create(schema: ColnSchema): AutomergeUrl {
    handle = repo.create(schema, colnDocType)
    return handle.url
  },

  async find(url: AutomergeUrl): Promise<void> {
    handle = await repo.find(url, colnDocType)
  },

  createTyped(): AutomergeUrl {
    const rawHandle = repo.create(itemFfi.schema, colnDocType)
    handle = rawHandle
    typedHandle = applyBindings(rawHandle, itemFfi)
    return typedHandle.url
  },

  applyTypedBindings(): void {
    typedHandle = applyBindings(currentHandle(), itemFfi)
  },

  add(path: string, values: Value[]): void {
    currentHandle().change(transaction => transaction.add(path, values))
  },

  rows(path: string): RowView[] {
    return currentHandle().doc().scanTable(path)
  },

  addTyped(value: string): void {
    currentTypedHandle().change(transaction => {
      transaction.root.Items(stringValue(value)).add()
    })
  },

  typedCount(value: string): number {
    const rows = currentTypedHandle().doc().root.Items(stringValue(value)).values()
    let count = 0
    for (let next = rows.next(); !next.done; next = rows.next()) count += 1
    return count
  },

  heads(): string[] {
    return currentHandle().doc().heads()
  },

  async flush(): Promise<void> {
    await repo.flush()
  },

  async shutdown(): Promise<void> {
    await repo.shutdown()
  },
}

function currentHandle(): RawHandle {
  if (!handle) throw new Error("no Coln store is open")
  return handle
}

function currentTypedHandle(): ItemHandle {
  if (!typedHandle) throw new Error("no typed Coln store is open")
  return typedHandle
}

function stringValue(value: string): Value {
  return { tag: "string", value }
}

declare global {
  interface Window {
    colnTest: typeof api
  }
}

window.colnTest = api
