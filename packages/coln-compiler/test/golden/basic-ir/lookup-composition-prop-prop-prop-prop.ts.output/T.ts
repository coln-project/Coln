import * as runtime from "@coln-project/runtime";

export interface View {
  A: runtime.ColnSet.View;
  B: runtime.ColnSet.View;
  C: runtime.ColnSet.View;
  E: (x: runtime.Value) => runtime.ColnSet.View;
  first: (x: runtime.Value) => runtime.ColnRef.View;
  second: (x: runtime.Value) => runtime.ColnRef.View;
  edge: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  A: runtime.ColnSet.Transaction;
  B: runtime.ColnSet.Transaction;
  C: runtime.ColnSet.Transaction;
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  first: (x: runtime.Value) => runtime.ColnRef.Transaction;
  second: (x: runtime.Value) => runtime.ColnRef.Transaction;
  edge: (x: runtime.Value) => runtime.ColnRef.Transaction;
}