import * as runtime from "@coln-project/runtime";
import * as Graph from "./Graph.ts";

class View {
  root: Graph.View;
  
  constructor(store: runtime.StoreHandle) {
    this.root = {
      vertex: new runtime.RowIdSet.View(store, "vertex", []),
      edge: (src: Value) => {
        runtime.assertStoreHas(store, "vertex", [], src, "root.vertex");
        return (tgt: Value) => {
          runtime.assertStoreHas(store, "vertex", [], tgt, "root.vertex");
          return new runtime.RowIdSet.View(store, "edge", [src, tgt]);
        }
      }
    }
  }
}

class Transaction extends View {
  root: Graph.Transaction;

  constructor(store: runtime.StoreHandle, transaction: runtime.TransactionHandle) {
    this.root = {
      vertex: new runtime.RowIdSet.Transaction(store, "vertex", [], transaction),
      edge: (src: Value) => {
        runtime.assertStoreHas(store, "vertex", [], src, "root.vertex");
        return (tgt: Value) => {
          runtime.assertStoreHas(store, "vertex", [], tgt, "root.vertex");
          return new runtime.RowIdSet.Transaction(store, "edge", [src, tgt], transaction);
        }
      }
    };
  }
}
