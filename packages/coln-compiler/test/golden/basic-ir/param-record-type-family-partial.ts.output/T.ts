import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  Y: runtime.ColnSet.View;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
  slices: (x: runtime.Value) => (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  Y: runtime.ColnSet.Transaction;
  R: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
  slices: (x: runtime.Value) => (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
}