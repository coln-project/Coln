// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

export type CommitHash = string;
export type WireRowId = { commit: CommitHash; counter: number };
export type TempRowId = { txId: number; counter: number };
export type TxnWireRowId =
  | { type: "Existing"; value: WireRowId }
  | { type: "Pending"; value: TempRowId };
export type WireValue = WireRowId | number | string;
export type WireTuple = WireValue[];
export type TxnWireValue = TxnWireRowId | number | string;
export type TxnWireTuple = TxnWireValue[];
export type LiveValue = RowId<string> | number | string;
export type LiveTuple = LiveValue[];

type RowIdState = TxnWireRowId | { type: "Canceled" };
const setRowIdState = Symbol();

export class RowId<Path extends string> {
  constructor(
    private state: RowIdState,
    readonly brand: Path,
  ) {}

  asTxnWire(): TxnWireRowId {
    if (this.state.type === "Canceled")
      throw new Error("row ID belongs to an aborted transaction");
    return this.state;
  }

  asWire(): WireRowId {
    const value = this.asTxnWire();
    if (value.type !== "Existing") {
      throw new Error("row ID has not been committed");
    }
    return value.value;
  }

  [setRowIdState](state: RowIdState): void {
    this.state = state;
  }
}

export function finishRowId(rowId: RowId<string>, commit: CommitHash): void {
  const state = rowId.asTxnWire();
  if (state.type === "Pending") {
    rowId[setRowIdState]({
      type: "Existing",
      value: { commit, counter: state.value.counter },
    });
  }
}

export function abortRowId(rowId: RowId<string>): void {
  if (rowId.asTxnWire().type === "Pending")
    rowId[setRowIdState]({ type: "Canceled" });
}

export function toTxnWireValue(value: LiveValue): TxnWireValue {
  return value instanceof RowId ? value.asTxnWire() : value;
}
