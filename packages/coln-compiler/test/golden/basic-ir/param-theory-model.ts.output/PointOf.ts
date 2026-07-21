import * as runtime from "@coln-project/runtime";
import * as Model from "./Model.ts";

export interface View {
  point: runtime.ColnRef.View;
}

export interface Transaction extends View {
  point: runtime.ColnRef.Transaction;
}