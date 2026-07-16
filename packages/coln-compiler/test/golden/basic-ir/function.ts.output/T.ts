import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  Y: runtime.ColnSet.View;
  next: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  Y: runtime.ColnSet.Transaction;
  next: (x: runtime.Value) => runtime.ColnRef.Transaction;
}