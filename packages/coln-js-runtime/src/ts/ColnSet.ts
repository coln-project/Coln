import { Value } from "./value.ts"
import { RowView  } from "#wasm-bodge/bindings";

export interface View {
  has(x: Value): boolean;
  values(): Iterator<RowView>;
}

export interface Transaction extends View {
  add(): Value;
}
