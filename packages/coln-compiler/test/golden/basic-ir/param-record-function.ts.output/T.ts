import * as runtime from "@coln-project/runtime";

export interface View {
  Key: runtime.ColnSet.View;
  f: (x: runtime.Value) => runtime.ColnRef.View;
  boxed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  Key: runtime.ColnSet.Transaction;
  f: (x: runtime.Value) => runtime.ColnRef.Transaction;
  boxed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}