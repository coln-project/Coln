import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  nested: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  nested: (x: runtime.Value) => runtime.ColnSet.Transaction;
}