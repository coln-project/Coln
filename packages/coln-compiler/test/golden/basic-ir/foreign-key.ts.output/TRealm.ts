import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      E: (a: Runtime.RowId<"root.V">) => Runtime.MutableSet<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        V: (new runtime.InductiveViewSet(mstore, "root.V", [])),
        E: (a: Runtime.RowId<"root.V">) => {
          return (new runtime.InductiveViewSet(mstore, "root.E", [a]));
        }
      };
    }
  };
}