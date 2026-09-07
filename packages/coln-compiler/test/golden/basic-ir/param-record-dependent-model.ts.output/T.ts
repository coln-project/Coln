import * as runtime from "@coln-project/runtime";
import * as PointOf from "./PointOf.ts";

export interface View {
  pointed: PointOf.View;
  boxed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  pointed: PointOf.Transaction;
  boxed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}