import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      E: (x: Runtime.RowId<"root.A">) => (a: null) => Runtime.MutableProp,
      next: (x: Runtime.RowId<"root.A">) => Runtime.Ref<null>,
      nextedge: (x: Runtime.RowId<"root.A">) => Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.InductiveViewSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.ViewProp(mstore, "root.B", [a]));
        },
        E: (x: Runtime.RowId<"root.A">) => {
          return (a: null) => {
            return (new runtime.ViewProp(mstore, "root.E", [x]));
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