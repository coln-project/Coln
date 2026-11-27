import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      E: (a: Runtime.RowId<"root.V">) => Runtime.MutableProp
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        V: (new runtime.InductiveViewSet(mstore, "root.V", [])),
        E: (a: Runtime.RowId<"root.V">) => {
          return (new runtime.ViewProp(mstore, "root.E", [a]));
        }
      };
    }
  };
}