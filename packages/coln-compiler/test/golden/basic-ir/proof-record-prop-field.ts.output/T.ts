import * as runtime from "@coln-project/runtime";

export interface View {
  P: runtime.ColnSet.View;
  evidence: Evidence.View;
}

export interface Transaction extends View {
  P: runtime.ColnSet.Transaction;
  evidence: Evidence.Transaction;
}