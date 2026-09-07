import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  P: (x: runtime.Value) => runtime.ColnSet.View;
  Q: (x: runtime.Value) => runtime.ColnSet.View;
  make: (x: runtime.Value) => Outer.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  P: (x: runtime.Value) => runtime.ColnSet.Transaction;
  Q: (x: runtime.Value) => runtime.ColnSet.Transaction;
  make: (x: runtime.Value) => Outer.Transaction;
}