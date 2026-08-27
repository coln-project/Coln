import { StoreHandle } from "#wasm-bodge/bindings"
import * as Set from "./Set"
import { Tuple } from "./tuple";
import { Value } from "./value";

class View implements Set.View {
  constructor(private store: StoreHandle, private path: string, private params: Tuple) {
    
  }
  
  has(v: Value): boolean {
  }
}
