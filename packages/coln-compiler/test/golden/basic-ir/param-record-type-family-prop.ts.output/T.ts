import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  P: (x: runtime.Value) => runtime.ColnSet.View;
  evidence: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  P: (x: runtime.Value) => runtime.ColnSet.Transaction;
  evidence: (x: runtime.Value) => (x: runtime.Value) => runtime.ColnSet.Transaction;
}