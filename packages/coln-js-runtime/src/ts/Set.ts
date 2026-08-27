import { Value } from "./value";

export interface View {
  has(x: Value): boolean;
  values(): Iterator<Value>
}

export interface Transaction extends View {
  add(): Value;
}
