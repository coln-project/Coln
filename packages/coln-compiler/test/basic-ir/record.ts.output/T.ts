import * as runtime from "@coln-project/runtime";

export interface View {
  point: runtime.ColnSet.View;
  payload: (x: runtime.Value) => Payload.View;
}

export interface Transaction extends View {
  point: runtime.ColnSet.Transaction;
  payload: (x: runtime.Value) => Payload.Transaction;
}