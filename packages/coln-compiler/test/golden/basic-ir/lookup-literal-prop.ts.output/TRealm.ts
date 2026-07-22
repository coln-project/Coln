import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      IntFact: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.IntFact", [a]));
      },
      StringFact: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.StringFact", [a]));
      },
      intFact: (new runtime.TableCellRef.View(store, "TRealm.intFact", [])),
      stringFact: (new runtime.TableCellRef.View(
        store,
        "TRealm.stringFact",
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
      IntFact: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.IntFact",
          [a],
          transaction
        ));
      },
      StringFact: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.StringFact",
          [a],
          transaction
        ));
      },
      intFact: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.intFact",
        [],
        transaction
      )),
      stringFact: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.stringFact",
        [],
        transaction
      ))
    };
  }
}