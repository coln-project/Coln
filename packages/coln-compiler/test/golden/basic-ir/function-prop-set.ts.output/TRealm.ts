import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableProp,
      Y: Runtime.MutableSet<Runtime.RowId<"root.Y">>,
      next: (a: null) => Runtime.Ref<Runtime.RowId<"root.Y">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.ViewProp(mstore, "root.X", [])),
        Y: (new runtime.InductiveViewSet(mstore, "root.Y", [])),
        next: (a: null) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.next",
            [],
            [0, 1],
            {
              flatten: (a: Runtime.RowId<"root.Y">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.Y"
                ));
              }
            }
          ));
        }
      };
    }
  };
}