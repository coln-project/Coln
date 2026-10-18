import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      E: (x: Runtime.RowId<"root.A">) => (a: null) => Runtime.MutableProp,
      next: (x: Runtime.RowId<"root.A">) => Runtime.MutableRef<null>,
      nextedge: (x: Runtime.RowId<"root.A">) => Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseProp(mstore, "root.B", [a]));
        },
        E: (x: Runtime.RowId<"root.A">) => {
          return (a: null) => {
            return (new runtime.BaseProp(mstore, "root.E", [x]));
          };
        },
        next: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.ConstRef(null));
        },
        nextedge: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}