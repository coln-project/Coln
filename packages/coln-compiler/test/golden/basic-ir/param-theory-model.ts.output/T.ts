import * as runtime from "@coln-project/runtime";
import * as Model from "./Model.ts";
import * as PointOf from "./PointOf.ts";

export interface View {
  model: Model.View;
  pointed: PointOf.View;
}

export interface Transaction extends View {
  model: Model.Transaction;
  pointed: PointOf.Transaction;
}