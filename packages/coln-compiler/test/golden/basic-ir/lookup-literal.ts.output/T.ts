import * as runtime from "@coln-project/runtime";

export interface View {
  IntEdge: (x: runtime.Value) => runtime.ColnSet.View;
  StringEdge: (x: runtime.Value) => runtime.ColnSet.View;
  intEdge: runtime.ColnRef.View;
  stringEdge: runtime.ColnRef.View;
}

export interface Transaction extends View {
  IntEdge: (x: runtime.Value) => runtime.ColnSet.Transaction;
  StringEdge: (x: runtime.Value) => runtime.ColnSet.Transaction;
  intEdge: runtime.ColnRef.Transaction;
  stringEdge: runtime.ColnRef.Transaction;
}