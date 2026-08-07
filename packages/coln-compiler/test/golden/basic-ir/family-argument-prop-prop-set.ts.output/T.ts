import * as runtime from "@coln-project/runtime";

export interface View {
  A: runtime.ColnSet.View;
  B: (x: runtime.Value) => runtime.ColnSet.View;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  A: runtime.ColnSet.Transaction;
  B: (x: runtime.Value) => runtime.ColnSet.Transaction;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
}