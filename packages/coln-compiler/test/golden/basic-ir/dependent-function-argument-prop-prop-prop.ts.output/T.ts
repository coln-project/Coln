import * as runtime from "@coln-project/runtime";

export interface View {
  A: runtime.ColnSet.View;
  B: (x: runtime.Value) => runtime.ColnSet.View;
  C: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
  f: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  A: runtime.ColnSet.Transaction;
  B: (x: runtime.Value) => runtime.ColnSet.Transaction;
  C: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
  f: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnRef.Transaction;
}