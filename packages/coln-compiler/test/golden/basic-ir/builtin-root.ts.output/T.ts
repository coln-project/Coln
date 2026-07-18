import * as runtime from "@coln-project/runtime";

export interface View {
  count: runtime.ColnRef.View;
  label: runtime.ColnRef.View;
}

export interface Transaction extends View {
  count: runtime.ColnRef.Transaction;
  label: runtime.ColnRef.Transaction;
}