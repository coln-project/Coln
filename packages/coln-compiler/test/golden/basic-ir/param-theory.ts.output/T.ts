import * as runtime from "@coln-project/runtime";
import * as PointOf from "./PointOf.ts";

export interface View {
  X: runtime.ColnSet.View;
  P: PointOf.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  P: PointOf.Transaction;
}