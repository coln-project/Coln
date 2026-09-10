import { WireTuple } from "./store";

export interface Adaptor<T> {
  flatten: (value: T) => WireTuple,
  reconstruct: (tuple: WireTuple) => T
}
