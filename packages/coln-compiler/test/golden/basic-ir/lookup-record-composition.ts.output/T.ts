import * as runtime from "@coln-project/runtime";

export interface View {
  X: runtime.ColnSet.View;
  key: (x: runtime.Value) => Key.View;
  PayloadAt: (x: runtime.Value) => runtime.ColnSet.View;
  slot: (x: runtime.Value) => runtime.ColnRef.View;
  payload: (x: runtime.Value) => (x: runtime.Value) => Payload.View;
  E: (x: runtime.Value) => runtime.ColnSet.View;
  edge: (x: runtime.Value) => runtime.ColnRef.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  key: (x: runtime.Value) => Key.Transaction;
  PayloadAt: (x: runtime.Value) => runtime.ColnSet.Transaction;
  slot: (x: runtime.Value) => runtime.ColnRef.Transaction;
  payload: (x: runtime.Value) => (x: runtime.Value) => Payload.Transaction;
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  edge: (x: runtime.Value) => runtime.ColnRef.Transaction;
}