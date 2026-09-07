import * as runtime from "@coln-project/runtime";

export interface View {
  payload: Outer.View;
}

export interface Transaction extends View {
  payload: Outer.Transaction;
}