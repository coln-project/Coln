// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  StoreHandle,
  type CommitChunk,
  type ColnSchema,
  type RealmBindings,
  type TransactionHandle,
} from "@coln-project/runtime"
import { defineDocumentType, type SedimentreeMeta } from "@automerge/automerge-repo/slim"

export type { ColnSchema, RealmBindings } from "@coln-project/runtime"

export type ColnState = {
  store: StoreHandle
  bindings?: RealmBindings
}

type ColnDocumentBase = Pick<StoreHandle, "heads" | "jsonIR" | "rowById" | "scanTable">
type ColnTransactionBase = Pick<TransactionHandle, "add">

export type ColnDocument<Bindings extends RealmBindings | undefined = undefined> =
  Bindings extends RealmBindings
    ? ColnDocumentBase & Pick<InstanceType<Bindings["View"]>, "root">
    : ColnDocumentBase

export type ColnTransaction<Bindings extends RealmBindings | undefined = undefined> =
  Bindings extends RealmBindings
    ? ColnTransactionBase & Pick<InstanceType<Bindings["Transaction"]>, "root">
    : ColnTransactionBase

export type ColnChange<Bindings extends RealmBindings | undefined = undefined> = (
  transaction: ColnTransaction<Bindings>,
) => void

export const colnDocType = defineDocumentType<
  ColnState,
  ColnDocumentBase,
  ColnChange,
  ColnSchema
>({
  name: "coln",
  empty: () => ({ store: StoreHandle.empty() }),
  init: schema => ({ store: StoreHandle.fromTheory(serializeSchema(schema)) }),
  view: createDocument,
  change: (state, change) => runTransaction(state, change),
  heads: state => state.store.heads(),
  hasData: state => state.store.heads().length > 0,
  sedimentree: {
    metadata: state => commitChunks(state).map(commitMetadata),
    materialize: (state, metadata) => {
      const wantedHeads = new Set(metadata.map(entry => entry.head))
      // TODO: Add a runtime commitChunksByHash API if full-history scans become costly.
      return commitChunks(state)
        .filter(chunk => wantedHeads.has(chunk.hash))
        .map(chunk => ({ ...commitMetadata(chunk), bytes: new Uint8Array(chunk.bytes) }))
    },
    apply: (state, blobs) => {
      if (blobs.length > 0) {
        // TODO: "Add independent StoreHandle snapshots for Repo document states":
        // apply blobs to a snapshot and return a new document.
        state.store.applyChunkBytes(blobs)
      }
      return state
    },
    liveHashes: state => commitChunks(state).map(chunk => chunk.hash),
  },
})

function createDocument(state: ColnState): ColnDocumentBase {
  const { store, bindings } = state
  const document: ColnDocumentBase & { root?: unknown } = {
    heads: () => store.heads(),
    jsonIR: () => store.jsonIR(),
    rowById: (path, rowId) => store.rowById(path, rowId),
    scanTable: path => store.scanTable(path),
  }
  if (bindings) document.root = new bindings.View(store).root
  return Object.freeze(document)
}

function createTransaction(
  state: ColnState,
  transaction: TransactionHandle,
): ColnTransactionBase {
  const { store, bindings } = state
  const exposed: ColnTransactionBase & { root?: unknown } = {
    add: (path, values) => transaction.add(path, values),
  }
  if (bindings) exposed.root = new bindings.Transaction(store, transaction).root
  return Object.freeze(exposed)
}

function runTransaction(document: ColnState, change: ColnChange): ColnState {
  // TODO: "Add independent StoreHandle snapshots for Repo document states":
  // start the transaction from a snapshot and leave this document unchanged.
  const { store, bindings } = document
  const transaction = store.beginTransaction()
  try {
    change(createTransaction(document, transaction))
    const commit = transaction.commit()
    return { store: commit.takeStore(), bindings }
  } catch (error) {
    // beginTransaction() moved ownership of the store into the transaction, and on
    // error we cannot return a fresh document. Recover the (aborted) store back onto
    // the existing state object, otherwise it would hold a dead StoreHandle.
    document.store = transaction.takeStore()
    throw error
  }
}

function serializeSchema(schema: ColnSchema): string {
  const serialized = JSON.stringify(schema)
  if (serialized === undefined) throw new Error("creating a Coln store requires a schema")
  return serialized
}

// TODO: Every call pays to serialize chunk bytes as number[] across the wasm
// boundary, even when only hashes/parents are needed (metadata, liveHashes).
// Switch to Uint8Array output once CommitChunk uses serde_bytes in the runtime.
function commitChunks(document: ColnState): CommitChunk[] {
  return document.store.commitChunksAfter([])
}

function commitMetadata(chunk: CommitChunk): SedimentreeMeta {
  return { kind: "commit", head: chunk.hash, parents: chunk.parents }
}
