import * as runtime from "@coln-project/runtime";

export interface View {
  Key: runtime.ColnSet.View;
  key: runtime.ColnRef.View;
  E: (x: runtime.Value) => runtime.ColnSet.View;
  f: (x: runtime.Value) => runtime.ColnRef.View;
  point: PointAt.View;
  boxed: (x: runtime.Value) => runtime.ColnSet.View;
}

export interface Transaction extends View {
  Key: runtime.ColnSet.Transaction;
  key: runtime.ColnRef.Transaction;
  E: (x: runtime.Value) => runtime.ColnSet.Transaction;
  f: (x: runtime.Value) => runtime.ColnRef.Transaction;
  point: PointAt.Transaction;
  boxed: (x: runtime.Value) => runtime.ColnSet.Transaction;
}