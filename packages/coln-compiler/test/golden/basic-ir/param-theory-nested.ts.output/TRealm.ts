import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as PointOf from "./PointOf.ts";
import * as Pointed from "./Pointed.ts";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      X: (new runtime.RowIdSet.View(store, "TRealm.X", [])),
      outer: {
        inner: {
          point: (new runtime.TableCellRef.View(
            store,
            "TRealm.outer.inner.point",
            []
          ))
        }
      }
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
      X: (new runtime.RowIdSet.Transaction(store, "TRealm.X", [], transaction)),
      outer: {
        inner: {
          point: (new runtime.TableCellRef.Transaction(
            store,
            "TRealm.outer.inner.point",
            [],
            transaction
          ))
        }
      }
    };
  }
}