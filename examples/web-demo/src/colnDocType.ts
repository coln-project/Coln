import {
  StoreHandle,
  type CommitChunk,
  type RowRef,
  type TransactionHandle,
  type Value,
} from "@coln-project/runtime"
import {
  defineDocumentType,
  type DocumentType,
  type DocumentTypeContext,
  type SedimentreeMeta,
} from "@automerge/automerge-repo/slim"

export type RowIdValue = Extract<Value, { tag: "row_id" }>

export interface ColnState {
  store: StoreHandle
  actor: string
}

export type ColnFfi<View, Transaction> = {
  schema: unknown
  View: new (store: StoreHandle) => View
  Transaction: new (store: StoreHandle, transaction: TransactionHandle) => Transaction
}

export type AnyColnFfi = ColnFfi<any, any>
export type ColnFfiView<Ffi extends AnyColnFfi> = Ffi extends ColnFfi<infer View, any> ? View : never
export type ColnFfiTransaction<Ffi extends AnyColnFfi> = Ffi extends ColnFfi<any, infer Transaction> ? Transaction : never

export type ColnDocument<Ffi extends AnyColnFfi> = {
  realm: ColnFfiView<Ffi>
  store: StoreHandle
  heads: string[]
}

export type ColnChange<Ffi extends AnyColnFfi> = (tx: ColnFfiTransaction<Ffi>) => void

export type ColnDocType<
  Ffi extends AnyColnFfi,
  View = ColnDocument<Ffi>,
  Change = ColnChange<Ffi>,
  Init = undefined,
> = DocumentType<ColnState, View, Change, Init>

export type ColnDocTypeOptions<
  Ffi extends AnyColnFfi,
  View = ColnDocument<Ffi>,
  Change = ColnChange<Ffi>,
  Init = undefined,
> = {
  name?: string
  view?: (doc: ColnDocument<Ffi>, state: ColnState) => View
  change?: (tx: ColnFfiTransaction<Ffi>, change: Change, state: ColnState) => void
  init?: (init: Init, tx: ColnFfiTransaction<Ffi>, state: ColnState) => void
  hasData?: (state: ColnState) => boolean
}

export function colnDocType<
  Ffi extends AnyColnFfi,
  View = ColnDocument<Ffi>,
  Change = ColnChange<Ffi>,
  Init = undefined,
>(
  ffi: Ffi,
  options: ColnDocTypeOptions<Ffi, View, Change, Init> = {}
): ColnDocType<Ffi, View, Change, Init> {
  const schemaJson = JSON.stringify(ffi.schema)
  const projectView = options.view ?? ((doc: ColnDocument<Ffi>) => doc as unknown as View)
  const applyChange =
    options.change ??
    ((tx: ColnFfiTransaction<Ffi>, change: Change) => {
      ;(change as unknown as ColnChange<Ffi>)(tx)
    })

  const makeState = (ctx: DocumentTypeContext): ColnState => ({
    store: StoreHandle.fromTheory(schemaJson),
    actor: ctx.peerId,
  })

  const makeDocument = (state: ColnState): ColnDocument<Ffi> => ({
    realm: new ffi.View(state.store),
    store: state.store,
    heads: state.store.heads(),
  })

  const runTransaction = (
    state: ColnState,
    body: (tx: ColnFfiTransaction<Ffi>) => void
  ): ColnState => {
    const tx = state.store.beginTransaction()
    try {
      const typedTx = new ffi.Transaction(state.store, tx)
      body(typedTx)
      const result = tx.commit()
      return { ...state, store: result.takeStore() }
    } catch (error) {
      try {
        state.store = tx.takeStore()
      } catch {
        // ignore; preserve the original error
      }
      throw error
    }
  }

  return defineDocumentType<ColnState, View, Change, Init>({
    name: options.name ?? "coln",
    empty: ctx => makeState(ctx),
    init: (init, ctx) => {
      const state = makeState(ctx)
      return options.init
        ? runTransaction(state, tx => options.init?.(init, tx, state))
        : state
    },
    view: state => projectView(makeDocument(state), state),
    change: (state, change) =>
      runTransaction(state, tx => applyChange(tx, change, state)),
    heads: state => state.store.heads(),
    hasData: state => options.hasData?.(state) ?? state.store.heads().length > 0,
    sedimentree: {
      metadata: state => chunks(state).map(chunkToMeta),
      materialize: (state, metas) => {
        const wanted = new Set(metas.map(meta => meta.head))
        return chunks(state)
          .filter(chunk => wanted.has(chunk.hash))
          .map(chunk => ({ ...chunkToMeta(chunk), bytes: new Uint8Array(chunk.bytes) }))
      },
      apply: (state, blobs) => {
        if (blobs.length > 0) {
          state.store.applyChunkBytes(blobs.map(blob => Array.from(blob)))
        }
        return state
      },
      liveHashes: state => chunks(state).map(chunk => chunk.hash),
    },
  })
}

export function rowValue(ref: RowRef | null): RowIdValue {
  if (!ref) throw new Error("cannot convert a synthetic row to a row_id value")
  return { tag: "row_id", value: ref }
}

export function refId(ref: RowRef): string {
  if ("existing" in ref) return `${ref.existing.commit}:${ref.existing.counter}`
  return `pending:${ref.pending.txId}:${ref.pending.counter}`
}

function chunks(state: ColnState): CommitChunk[] {
  return state.store.commitChunksAfter([])
}

function chunkToMeta(chunk: CommitChunk): SedimentreeMeta {
  return { kind: "commit", head: chunk.hash, parents: chunk.parents }
}
