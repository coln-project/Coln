import { CommitHash, TxnWireRowId, WireRowId } from "./types.js";
import { WireValue } from "./value.js"

type RowIdInner = TxnWireRowId | { type: "Canceled", counter: number }

export class RowId<S extends string> {
  constructor(private rowId: RowIdInner, readonly brand: S) {}
  
  asWire(): WireRowId {
    if (this.rowId.type == "Existing") {
      return this.rowId.value
    } else if (this.rowId.type == "Pending") {
      throw "Must commit before using this row id in a query"
    } else if (this.rowId.type == "Canceled") {
      throw "This row id is from a transaction that was aborted"
    } else {
      throw "Unknown row id type"
    }
  }
  
  asTxnWire(): TxnWireRowId {
    if (this.rowId.type != "Canceled") {
      return this.rowId
    } else {
      throw "This row id is from a transaction that was aborted"
    }
  }
  
  finish(commit: CommitHash) {
    if (this.rowId.type == "Pending") {
      const counter = this.rowId.value
      this.rowId = {
        type: "Existing",
        value: {commit, counter}
      }
    }
  }
  
  abort() {
    if (this.rowId.type == "Pending") {
      this.rowId = { type: "Canceled", counter: this.rowId.value }
    }
  }
}

export function rowIdFromWire<P extends string>(v: WireValue, brand: P): RowId<P> {
  if (typeof(v) == "number") {
    throw "expected row id"
  } else if (typeof(v) == "string") {
    throw "expected row id"
  } else {
    return new RowId({ type: "Existing", value: v }, brand)
  }
}
