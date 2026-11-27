import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      R: (a: null) => (b: Runtime.RowId<"root.B">) => Runtime.MutableProp
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.ViewProp(mstore, "root.A", [])),
        B: (a: null) => {
          return (new runtime.InductiveViewSet(mstore, "root.B", []));
        },
        R: (a: null) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.ViewProp(mstore, "root.R", [b]));
          };
        }
      };
    }
  };
}