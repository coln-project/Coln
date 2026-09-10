import { WireTuple } from "./store";

export class Adaptor<T> {
  constructor(public flatten: (value: T) => WireTuple, public reconstruct: (tuple: WireTuple) => T) {}
}
