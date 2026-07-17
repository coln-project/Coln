import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  Pair: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  Pair: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
}