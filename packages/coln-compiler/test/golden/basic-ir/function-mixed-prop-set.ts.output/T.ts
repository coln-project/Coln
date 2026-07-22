import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  P: runtime.ColnSet.View;
  Y: runtime.ColnSet.View;
  choose: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  P: runtime.ColnSet.Transaction;
  Y: runtime.ColnSet.Transaction;
  choose: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnRef.Transaction;
}