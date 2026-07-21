import * as runtime from "@coln-project/runtime";
import * as Model from "./Model.ts";

export interface View {
  model: Model.View;
  boxed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  model: Model.Transaction;
  boxed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}