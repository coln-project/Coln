import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (x: null) => (a: Runtime.RowId<"root.B">) => Runtime.MutableProp,
      next: (x: null) => Runtime.Ref<Runtime.RowId<"root.B">>,
      nextedge: (x: null) => Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.ViewProp(mstore, "root.A", [])),
        B: (a: null) => {
          return (new runtime.InductiveViewSet(mstore, "root.B", []));
        },
        E: (x: null) => {
          return (a: Runtime.RowId<"root.B">) => {
            return (new runtime.ViewProp(mstore, "root.E", [a]));
          };
        },
        next: (x: null) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.next",
            [],
            [0, 1],
            {
              flatten: (a: Runtime.RowId<"root.B">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.B"
                ));
              }
            }
          ));
        },
        nextedge: (x: null) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}