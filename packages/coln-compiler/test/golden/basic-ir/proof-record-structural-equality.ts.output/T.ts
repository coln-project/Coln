import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  comparison: Comparison.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  comparison: Comparison.Transaction;
}