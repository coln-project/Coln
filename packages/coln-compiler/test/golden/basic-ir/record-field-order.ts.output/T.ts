import * as runtime from "@coln-project/runtime";

export interface View {
  E: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  E: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
}