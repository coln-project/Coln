import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  payload: (x: runtime.Value) => Payload.View;
  E: (x: runtime.Value) => runtime.ColnSet.View;
  edge: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  payload: (x: runtime.Value) => Payload.Transaction;
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  edge: (x: runtime.Value) => runtime.ColnRef.Transaction;
}