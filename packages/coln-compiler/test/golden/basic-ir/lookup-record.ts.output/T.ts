import * as runtime from "@coln-project/runtime";

export interface View {
  E: (x: runtime.Value) => runtime.ColnSet.View;
  selected: runtime.ColnRef.View;
}

export interface Transaction extends View {
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  selected: runtime.ColnRef.Transaction;
}