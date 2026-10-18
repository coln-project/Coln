import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      R: (a: null) => (b: null) => Runtime.MutableSet<Runtime.RowId<"root.R">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseProp(mstore, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseProp(mstore, "root.B", []));
        },
        R: (a: null) => {
          return (b: null) => {
            return (new runtime.BaseSet(mstore, "root.R", []));
          };
        }
      };
    }
  };
}