import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  result: (x: runtime.Value) => Result.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  result: (x: runtime.Value) => Result.Transaction;
}