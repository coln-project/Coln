import * as runtime from "@coln-project/runtime";

export interface View {
  P: (x: runtime.Value) => runtime.ColnSet.View;
  package: Package.View;
}

export interface Transaction extends View {
  P: (x: runtime.Value) => runtime.ColnSet.Transaction;
  package: Package.Transaction;
}