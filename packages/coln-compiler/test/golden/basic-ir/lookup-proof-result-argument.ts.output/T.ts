import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  P: (x: runtime.Value) => runtime.ColnSet.View;
  witness: (x: runtime.Value) => runtime.ColnRef.View;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
  use: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  P: (x: runtime.Value) => runtime.ColnSet.Transaction;
  witness: (x: runtime.Value) => runtime.ColnRef.Transaction;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
  use: (x: runtime.Value) => runtime.ColnRef.Transaction;
}