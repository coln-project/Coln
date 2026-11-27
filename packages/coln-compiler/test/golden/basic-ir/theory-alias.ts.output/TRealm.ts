import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: { X: Runtime.MutableSet<Runtime.RowId<"root.X">> };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = { X: (new runtime.BaseSet(mbouncer, "root.X", [])) };
    }
  };
}