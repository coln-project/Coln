import * as runtime from "@coln-project/runtime";

export interface View {
  IntFact: (x: runtime.Value) => runtime.ColnSet.View;
  StringFact: (x: runtime.Value) => runtime.ColnSet.View;
  intFact: runtime.ColnRef.View;
  stringFact: runtime.ColnRef.View;
}

export interface Transaction extends View {
  IntFact: (x: runtime.Value) => runtime.ColnSet.Transaction;
  StringFact: (x: runtime.Value) => runtime.ColnSet.Transaction;
  intFact: runtime.ColnRef.Transaction;
  stringFact: runtime.ColnRef.Transaction;
}