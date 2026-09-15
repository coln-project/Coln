// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

export type { CommitChunk, RowRef, RowView, Value } from "#wasm-bodge/bindings";
export { CommitResult, StoreHandle, TransactionHandle, valueEqual } from "#wasm-bodge/bindings"

export type { RealmBindings, ColnSchema } from "./RealmBindings"

export * as ColnSet from "./ColnSet";

export * as ColnRef from "./ColnRef";

export * as RowIdSet from "./RowIdSet"

export * as TableCellRef from "./TableCellRef";

export { ManagedStore } from "./BindingStore.js";
export type {
  ManagedStoreBackend,
  WhereClause,
} from "./BindingStore.js";
export {
  managedReadBackend,
  managedTransactionBackend,
} from "./WasmManagedStoreBackend.js";
export { BaseTableSet, ViewTableSet } from "./BindingSet.js";
export type { MutableSet, Set } from "./BindingSet.js";
export { RowId } from "./BindingValue.js";
export type {
  TxnWireRowId,
  TxnWireTuple,
  TxnWireValue,
  WireRowId,
  WireTuple,
  WireValue,
} from "./BindingValue.js";
