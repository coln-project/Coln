import * as runtime from "@coln-project/runtime";

export interface View {
  truth: Truth.View;
}

export interface Transaction extends View {
  truth: Truth.Transaction;
}