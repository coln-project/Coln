import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: { V: Runtime.MutableSet<Runtime.RowId<"root.V">> };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = { V: (new runtime.BaseSet(mbouncer, "root.V", [])) };
    }
  };
}