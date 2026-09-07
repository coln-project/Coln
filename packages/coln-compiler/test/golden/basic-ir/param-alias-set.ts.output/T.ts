import * as runtime from "@coln-project/runtime";
import * as SetAlias from "./SetAlias.ts";

export interface View {
  X: runtime.ColnSet.View;
  boxed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  boxed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}