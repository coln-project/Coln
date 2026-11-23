import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      R: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableSet<Runtime.RowId<"root.R">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.InductiveViewSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.ViewProp(mstore, "root.B", [a]));
        },
        R: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.InductiveViewSet(mstore, "root.R", [a]));
          };
        }
      };
    }
  };
}