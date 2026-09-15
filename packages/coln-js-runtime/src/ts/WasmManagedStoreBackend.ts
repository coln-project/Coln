// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  type RowView,
  StoreHandle,
  type TransactionHandle,
  type Value,
  valueEqual,
} from "#wasm-bodge/bindings";
import type {
  ManagedStoreBackend,
  WhereClause,
} from "./BindingStore.js";
import type {
  TxnWireRowId,
  TxnWireTuple,
  WireRowId,
  WireTuple,
  WireValue,
} from "./BindingValue.js";

export function managedReadBackend(
  store: StoreHandle,
): ManagedStoreBackend<StoreHandle> {
  return new WasmReadBackend(store);
}

export function managedTransactionBackend(
  store: StoreHandle,
): ManagedStoreBackend<StoreHandle> {
  return new WasmTransactionBackend(store.beginTransaction());
}

class WasmReadBackend implements ManagedStoreBackend<StoreHandle> {
  constructor(private readonly store: StoreHandle) {}

  all_proj(query: WhereClause, select: number[]): WireTuple[] {
    return this.rows(query).map((row) =>
      select.map((index) => fromWasmValue(row.values[index])),
    );
  }

  all_row_id(query: WhereClause): WireRowId[] {
    return this.rows(query).map((row) => {
      const value = fromWasmValue(row.rowId);
      if (typeof value === "number" || typeof value === "string")
        throw new TypeError("store returned a non-row ID");
      return value;
    });
  }

  one_proj(query: WhereClause, select: number[]): WireTuple {
    const rows = this.all_proj(query, select);
    if (rows.length !== 1)
      throw new Error(`expected one row, found ${rows.length}`);
    return rows[0];
  }

  exists(query: WhereClause): boolean {
    return this.rows(query).length > 0;
  }

  add(_path: string, _values: TxnWireTuple): TxnWireRowId {
    throw new Error("store is not in an active change");
  }

  commit(): { commit: string; store: StoreHandle } {
    throw new Error("store is not in an active change");
  }

  abort(): StoreHandle {
    throw new Error("store is not in an active change");
  }

  private rows(query: WhereClause): RowView[] {
    // The WASM backend currently exposes physical scans only. A native backend
    // can resolve the same logical relation name through its query engine.
    return this.store.scanTable(query.table_name).filter((row) => {
      if (query.row_id && !valueEqual(row.rowId, toWasmValue(query.row_id)))
        return false;
      return query.values.every((value, index) =>
        valueEqual(row.values[index], toWasmValue(value)),
      );
    });
  }
}

class WasmTransactionBackend
  implements ManagedStoreBackend<StoreHandle>
{
  constructor(private readonly transaction: TransactionHandle) {}

  all_proj(_query: WhereClause, _select: number[]): WireTuple[] {
    throw new Error("reads are unavailable during a change");
  }

  all_row_id(_query: WhereClause): WireRowId[] {
    throw new Error("reads are unavailable during a change");
  }

  one_proj(_query: WhereClause, _select: number[]): WireTuple {
    throw new Error("reads are unavailable during a change");
  }

  exists(_query: WhereClause): boolean {
    throw new Error("reads are unavailable during a change");
  }

  add(path: string, values: TxnWireTuple): TxnWireRowId {
    return fromWasmRowId(
      this.transaction.add(path, values.map(toWasmTransactionValue)),
    );
  }

  commit(): { commit: string; store: StoreHandle } {
    const result = this.transaction.commit();
    return { commit: result.commit, store: result.takeStore() };
  }

  abort(): StoreHandle {
    return this.transaction.takeStore();
  }
}

function fromWasmValue(value: Value): WireValue {
  if (value.tag === "int" || value.tag === "string") return value.value;
  if ("existing" in value.value) return value.value.existing;
  throw new Error("store returned a pending row ID");
}

function fromWasmRowId(value: Value): TxnWireRowId {
  if (value.tag !== "row_id") throw new TypeError("store returned a non-row ID");
  return "existing" in value.value
    ? { type: "Existing", value: value.value.existing }
    : { type: "Pending", value: value.value.pending };
}

function toWasmValue(value: WireValue): Value {
  if (typeof value === "number") return { tag: "int", value };
  if (typeof value === "string") return { tag: "string", value };
  return { tag: "row_id", value: { existing: value } };
}

function toWasmTransactionValue(
  value: TxnWireRowId | number | string,
): Value {
  if (typeof value === "number" || typeof value === "string")
    return toWasmValue(value);
  return value.type === "Existing"
    ? { tag: "row_id", value: { existing: value.value } }
    : { tag: "row_id", value: { pending: value.value } };
}
