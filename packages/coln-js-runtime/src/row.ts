import type {
  RowId,
  RowView,
  CellValue,
} from "#wasm-bodge/bindings";

import {
  RowHandle,
  TxnValue,
} from "#wasm-bodge/bindings";

export type { RowId, RowView, CellValue };
export { RowHandle, TxnValue };

// Write-side value: a row reference is an opaque `RowHandle` because its
// committed `RowId` may not exist yet while a transaction is still open.
// Ints/strings are plain data. This is what relations are parameterised by and
// what `add()` returns.
export type Value =
  | { tag: "row_id"; value: RowHandle }
  | { tag: "int"; value: number }
  | { tag: "string"; value: string };

// Either a write-side `Value` (row ref is a `RowHandle`) or a read-side
// `CellValue` returned from the store (row ref is a committed `RowId`). Both can
// be compared once row refs are resolved to a `RowId`.
export type AnyValue = Value | CellValue;

// Bridge a write-side `Value` into the opaque `TxnValue` the store expects.
export function toTxnValue(v: Value): TxnValue {
  switch (v.tag) {
    case "row_id":
      return TxnValue.row(v.value);
    case "int":
      return TxnValue.int(BigInt(v.value));
    case "string":
      return TxnValue.string(v.value);
  }
}

export function valueEqual(v0: AnyValue, v1: AnyValue): boolean {
  if (v0.tag === "row_id" && v1.tag === "row_id") {
    const r0 = resolveRowId(v0.value);
    const r1 = resolveRowId(v1.value);
    return r0 !== undefined && r1 !== undefined && rowIdEqual(r0, r1);
  } else if (v0.tag === "int" && v1.tag === "int") {
    return v0.value === v1.value;
  } else if (v0.tag === "string" && v1.tag === "string") {
    return v0.value === v1.value;
  }
  return false;
}

// A row ref is a live `RowHandle` (write side) or an already-committed `RowId`
// (read side); collapse both to a `RowId`. A handle for a still-pending row has
// no committed id yet, so it resolves to `undefined`.
function resolveRowId(value: RowHandle | RowId): RowId | undefined {
  return value instanceof RowHandle ? value.tryRowId() : value;
}

function rowIdEqual(v0: RowId, v1: RowId): boolean {
  return v0.commit === v1.commit && v0.counter === v1.counter;
}
