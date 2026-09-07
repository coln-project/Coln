import * as runtime from "@coln-project/runtime";

export interface View {
  point: runtime.ColnRef.View;
}

export interface Transaction extends View {
  point: runtime.ColnRef.Transaction;
}