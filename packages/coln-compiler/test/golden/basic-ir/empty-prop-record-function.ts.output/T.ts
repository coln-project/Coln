import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  trivial: (x: runtime.Value) => Truth.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  trivial: (x: runtime.Value) => Truth.Transaction;
}