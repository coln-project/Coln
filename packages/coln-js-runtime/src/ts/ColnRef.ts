import { Value } from "#wasm-bodge/bindings";

export interface View {
  get(): Value
}

export interface Transaction extends View {
  set(v: Value): void;
}
