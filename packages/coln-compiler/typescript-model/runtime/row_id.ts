import { CommitHash, TxnWireRowId, WireRowId } from "./types.js";

export class RowId<S extends string> {
  constructor(private rowId: TxnWireRowId, readonly brand: S) {}
  
  asWire(): WireRowId {
    if (this.rowId.type == "Existing") {
      return this.rowId.value
    } else if (this.rowId.type == "Pending") {
      throw "Must commit before using this row id in a query"
    } else {
      throw "Unknown row id type"
    }
  }
  
  asTxnWire(): TxnWireRowId {
    return this.rowId
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
}
