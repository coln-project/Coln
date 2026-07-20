import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  P: runtime.ColnSet.View;
  Q: runtime.ColnSet.View;
  R: (x: runtime.Value) => (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  P: runtime.ColnSet.Transaction;
  Q: runtime.ColnSet.Transaction;
  R: (x: runtime.Value) => (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
}