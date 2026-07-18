import * as runtime from "@coln-project/runtime";

export interface View {
  V: runtime.ColnSet.View;
}

export interface Transaction extends View {
  V: runtime.ColnSet.Transaction;
}