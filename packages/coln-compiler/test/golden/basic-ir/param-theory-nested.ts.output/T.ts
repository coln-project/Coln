import * as runtime from "@coln-project/runtime";
import * as PointOf from "./PointOf.ts";
import * as Pointed from "./Pointed.ts";

export interface View {
  X: runtime.ColnSet.View;
  outer: Pointed.View;
}

export interface Transaction extends View {
  X: runtime.ColnSet.Transaction;
  outer: Pointed.Transaction;
}