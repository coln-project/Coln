import * as runtime from "@coln-project/runtime";

export interface View {
  boxed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  boxed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}