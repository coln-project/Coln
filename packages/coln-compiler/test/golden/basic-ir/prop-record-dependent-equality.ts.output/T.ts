import * as runtime from "@coln-project/runtime";

export interface View {
  P: runtime.ColnSet.View;
  witness: Witness.View;
}

export interface Transaction extends View {
  P: runtime.ColnSet.Transaction;
  witness: Witness.Transaction;
}