import type { RowId, RowView, CellValue } from "#wasm-bodge/bindings";

import { RowHandle, TxnValue } from "#wasm-bodge/bindings";

export type { RowId, RowView, CellValue };
export { RowHandle, TxnValue };

// Either a write-side `Value` (row ref is a `RowHandle`) or a read-side
// `CellValue` returned from the store (row ref is a committed `RowId`). Both can
// be compared once row refs are resolved to a `RowId`.
export type Value =
  | { tag: "row_handle"; value: RowHandle }
  | { tag: "row_id"; value: RowId }
  | { tag: "int"; value: number }
  | { tag: "string"; value: string };

export function valueRowId(v: Value): RowId {
  switch (v.tag) {
    case "row_id":
      return v.value;
    case "row_handle":
      return v.value.rowId();
    case "int":
    case "string":
      throw new Error("int or string has no rowid");
  }
}

export function tryValueRowId(v: Value): RowId | undefined {
  try {
    return valueRowId(v);
  } catch (e) {
    return undefined;
  }
}

// Bridge a write-side `Value` into the opaque `TxnValue` the store expects.
export function toTxnValue(v: Value): TxnValue {
  switch (v.tag) {
    case "row_id":
      return TxnValue.rowId(v.value);
    case "row_handle":
      return TxnValue.row(v.value);
    case "int":
      return TxnValue.int(BigInt(v.value));
    case "string":
      return TxnValue.string(v.value);
  }
}

export function valueEqual(v0: Value, v1: Value): boolean {
  const isRowRef = (t: Value["tag"]) => t === "row_id" || t === "row_handle";
  if (isRowRef(v0.tag) && isRowRef(v1.tag)) {
    const r0 = tryValueRowId(v0);
    const r1 = tryValueRowId(v1);
    return r0 !== undefined && r1 !== undefined && rowIdEqual(r0, r1);
  } else if (v0.tag === "int" && v1.tag === "int") {
    return v0.value === v1.value;
  } else if (v0.tag === "string" && v1.tag === "string") {
    return v0.value === v1.value;
  }
  return false;
}

function rowIdEqual(v0: RowId, v1: RowId): boolean {
  return v0.commit === v1.commit && v0.counter === v1.counter;
}
