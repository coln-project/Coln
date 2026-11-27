import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      E: (x: Runtime.RowId<"root.A">) => (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      next: (x: Runtime.RowId<"root.A">) => Runtime.Ref<null>,
      nextedge: (x: Runtime.RowId<"root.A">) => Runtime.Ref<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.InductiveViewSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.ViewProp(mstore, "root.B", [a]));
        },
        E: (x: Runtime.RowId<"root.A">) => {
          return (a: null) => {
            return (new runtime.InductiveViewSet(mstore, "root.E", [x]));
          };
        },
        next: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.ConstRef(null));
        },
        nextedge: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.nextedge",
            [x],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.E">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.E"
                ));
              }
            }
          ));
        }
      };
    }
  };
}