import * as runtime from "@coln-project/runtime";

export interface View {
  E: (x: runtime.Value) => runtime.ColnSet.View;
  boxed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  boxed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}