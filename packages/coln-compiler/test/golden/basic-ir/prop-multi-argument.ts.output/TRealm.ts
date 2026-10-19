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
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        P: (new runtime.BaseProp(mstore, "root.P", [])),
        Q: (new runtime.BaseProp(mstore, "root.Q", [])),
        R: (a: Runtime.RowId<"root.X">) => {
          return (b: null) => {
            return (c: null) => {
              return (new runtime.BaseProp(mstore, "root.R", [a]));
            };
          };
        }
      };
    }
  };
}