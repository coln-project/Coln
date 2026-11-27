import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      E: (x: null) => (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      next: (x: null) => Runtime.Ref<null>,
      nextedge: (x: null) => Runtime.Ref<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.ViewProp(mstore, "root.A", [])),
        B: (a: null) => {
          return (new runtime.ViewProp(mstore, "root.B", []));
        },
        E: (x: null) => {
          return (a: null) => {
            return (new runtime.InductiveViewSet(mstore, "root.E", []));
          };
        },
        next: (x: null) => {
          return (new runtime.ConstRef(null));
        },
        nextedge: (x: null) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.nextedge",
            [],
            [0, 1],
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