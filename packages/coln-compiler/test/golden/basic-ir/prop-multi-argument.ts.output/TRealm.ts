import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      P: Runtime.MutableProp,
      Q: Runtime.MutableProp,
      R: (a: Runtime.RowId<"root.X">) => (b: null) => (c: null) => Runtime.MutableProp
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.InductiveViewSet(mstore, "root.X", [])),
        P: (new runtime.ViewProp(mstore, "root.P", [])),
        Q: (new runtime.ViewProp(mstore, "root.Q", [])),
        R: (a: Runtime.RowId<"root.X">) => {
          return (b: null) => {
            return (c: null) => {
              return (new runtime.ViewProp(mstore, "root.R", [a]));
            };
          };
        }
      };
    }
  };
}