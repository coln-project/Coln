import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  value: EqualTriple.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  value: EqualTriple.Transaction;
}