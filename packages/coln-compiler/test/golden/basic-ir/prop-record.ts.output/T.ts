import * as runtime from "@coln-project/runtime";

export interface View {
  P: runtime.ColnSet.View;
  Q: runtime.ColnSet.View;
  make: (x: runtime.Value) => (x: runtime.Value) => And.View;
  projectLeft: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  P: runtime.ColnSet.Transaction;
  Q: runtime.ColnSet.Transaction;
  make: (x: runtime.Value) => (x: runtime.Value) => And.Transaction;
  projectLeft: (x: runtime.Value) => runtime.ColnRef.Transaction;
}