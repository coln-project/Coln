// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { AutomergeUrl, Repo } from "@automerge/automerge-repo"
import type { Value } from "@coln-project/runtime"
import {
  colnDocType,
  find,
  type ColnChange,
  type ColnDocument,
  type ColnHandle,
  type ColnTransaction,
  type RealmBindings,
} from "@coln-project/repo"

type Equal<Left, Right> =
  (<Type>() => Type extends Left ? 1 : 2) extends <Type>() => Type extends Right ? 1 : 2
    ? true
    : false
type Expect<Type extends true> = Type

declare const repo: Repo
declare const url: AutomergeUrl
declare const bindings: RealmBindings
declare const optionalBindings: RealmBindings | undefined

const raw = find(repo, url)
const explicitUndefined = find(repo, url, undefined)
const typed = find(repo, url, bindings)
const optional = find(repo, url, optionalBindings)

void raw.then(handle => {
  handle.doc().scanTable("Example.Items")
  handle.fullDoc().store.heads()
})

void typed.then(handle => {
  handle.doc().root
  handle.on("change", payload => {
    payload.doc?.root
    payload.handle.doc().root
  })
  handle.addListener("change", payload => payload.doc?.root)
  handle.listeners("change").forEach(listener => listener)
  handle.on("heads-changed", payload => {
    payload.doc.store
    payload.handle.doc().root
  })
  handle.change(transaction => {
    const value: Value = transaction.add("Example.Items", [])
    value
    transaction.root
  })
})

type DocumentOperation = "heads" | "jsonIR" | "rowById" | "scanTable"

export type TypeChecks = [
  Expect<Equal<typeof raw, Promise<ColnHandle>>>,
  Expect<Equal<typeof explicitUndefined, Promise<ColnHandle>>>,
  Expect<Equal<typeof typed, Promise<ColnHandle<typeof bindings>>>>,
  Expect<Equal<typeof optional, Promise<ColnHandle | ColnHandle<typeof bindings>>>>,
  Expect<Equal<ReturnType<ColnHandle<typeof bindings>["doc"]>, ColnDocument<typeof bindings>>>,
  Expect<
    Equal<Parameters<ColnHandle<typeof bindings>["change"]>[0], ColnChange<typeof bindings>>
  >,
  Expect<Equal<Parameters<ColnChange<typeof bindings>>[0], ColnTransaction<typeof bindings>>>,
  Expect<Equal<keyof ColnDocument, DocumentOperation>>,
  Expect<Equal<keyof ColnDocument<typeof bindings>, DocumentOperation | "root">>,
  Expect<Equal<keyof ColnTransaction, "add">>,
  Expect<Equal<keyof ColnTransaction<typeof bindings>, "add" | "root">>,
]
