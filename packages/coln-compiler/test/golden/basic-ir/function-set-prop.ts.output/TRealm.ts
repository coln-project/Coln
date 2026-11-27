import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Y: Runtime.MutableProp,
      next: (a: Runtime.RowId<"root.X">) => Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.InductiveViewSet(mstore, "root.X", [])),
        Y: (new runtime.ViewProp(mstore, "root.Y", [])),
        next: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}