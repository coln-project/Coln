import * as runtime from "@coln-project/runtime";

export interface View {
  unit: Unit.View;
}

export interface Transaction extends View {
  unit: Unit.Transaction;
}