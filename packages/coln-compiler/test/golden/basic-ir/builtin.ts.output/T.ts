import * as runtime from "@coln-project/runtime";

export interface View {
  V: runtime.ColnSet.View;
  count: (x: runtime.Value) => runtime.ColnRef.View;
  label: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  V: runtime.ColnSet.Transaction;
  count: (x: runtime.Value) => runtime.ColnRef.Transaction;
  label: (x: runtime.Value) => runtime.ColnRef.Transaction;
}