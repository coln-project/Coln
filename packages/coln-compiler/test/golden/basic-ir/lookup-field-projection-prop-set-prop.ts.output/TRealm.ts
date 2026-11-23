import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (a: Runtime.RowId<"root.B">) => Runtime.MutableProp,
      x: Runtime.Ref<null>,
      next: (a: null) => Runtime.Ref<Runtime.RowId<"root.B">>,
      edge: Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.ViewProp(mstore, "root.A", [])),
        B: (new runtime.InductiveViewSet(mstore, "root.B", [])),
        E: (a: Runtime.RowId<"root.B">) => {
          return (new runtime.ViewProp(mstore, "root.E", [a]));
        },
        x: (new runtime.ConstRef(null)),
        next: (a: null) => {
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
        edge: (new runtime.ConstRef(null))
      };
    }
  };
}