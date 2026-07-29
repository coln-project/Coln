import * as runtime from "@coln-project/runtime";

export interface View {
  A: runtime.ColnSet.View;
  B: runtime.ColnSet.View;
  E: (x: runtime.Value) => runtime.ColnSet.View;
  next: (x: runtime.Value) => runtime.ColnRef.View;
  nextedge: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  A: runtime.ColnSet.Transaction;
  B: runtime.ColnSet.Transaction;
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  next: (x: runtime.Value) => runtime.ColnRef.Transaction;
  nextedge: (x: runtime.Value) => runtime.ColnRef.Transaction;
}