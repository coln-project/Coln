import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  rank: (x: runtime.Value) => runtime.ColnRef.View;
  E: (x: runtime.Value) => runtime.ColnSet.View;
  edge: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  rank: (x: runtime.Value) => runtime.ColnRef.Transaction;
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  edge: (x: runtime.Value) => runtime.ColnRef.Transaction;
}