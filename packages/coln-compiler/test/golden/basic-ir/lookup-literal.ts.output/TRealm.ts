import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      IntEdge: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.IntEdge", [a]));
      },
      StringEdge: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.StringEdge", [a]));
      },
      intEdge: (new runtime.TableCellRef.View(store, "TRealm.intEdge", [])),
      stringEdge: (new runtime.TableCellRef.View(
        store,
        "TRealm.stringEdge",
        []
      ))
    };
  }
}

export class Transaction extends View {
  root: T.Transaction;

  constructor(
    store: runtime.StoreHandle,
    transaction: runtime.TransactionHandle
  ) {
    super(store);
    this.root = {
      IntEdge: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.IntEdge",
          [a],
          transaction
        ));
      },
      StringEdge: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.StringEdge",
          [a],
          transaction
        ));
      },
      intEdge: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.intEdge",
        [],
        transaction
      )),
      stringEdge: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.stringEdge",
        [],
        transaction
      ))
    };
  }
}