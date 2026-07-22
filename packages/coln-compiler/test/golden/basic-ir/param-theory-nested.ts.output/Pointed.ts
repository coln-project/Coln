import * as runtime from "@coln-project/runtime";
import * as PointOf from "./PointOf.ts";

export interface View {
  inner: PointOf.View;
}

export interface Transaction extends View {
  inner: PointOf.Transaction;
}