import * as runtime from "@coln-project/runtime";

export interface View {
  E: (x: runtime.Value) => runtime.ColnSet.View;
  r: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  r: (x: runtime.Value) => runtime.ColnRef.Transaction;
}