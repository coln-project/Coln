import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: { X: Runtime.MutableSet<Runtime.RowId<"root.X">> };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = { X: (new runtime.BaseSet(mstore, "root.X", [])) };
    }
  };
}