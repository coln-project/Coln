import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  witness: (x: runtime.Value) => Witness.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  witness: (x: runtime.Value) => Witness.Transaction;
}