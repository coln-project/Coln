import * as runtime from "@coln-project/runtime";

export interface View {
  A: runtime.ColnSet.View;
  B: runtime.ColnSet.View;
  shadowed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  A: runtime.ColnSet.Transaction;
  B: runtime.ColnSet.Transaction;
  shadowed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}