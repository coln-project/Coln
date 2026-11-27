import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: { V: Runtime.MutableProp };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = { V: (new runtime.BaseProp(mbouncer, "root.V", [])) };
    }
  };
}