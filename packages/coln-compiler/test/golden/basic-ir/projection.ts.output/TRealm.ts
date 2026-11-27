import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      E: (a: Runtime.RowId<"root.X">) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      r: (p: {
        first: Runtime.RowId<"root.X">,
        second: Runtime.RowId<"root.X">
      }) => Runtime.Ref<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.InductiveViewSet(mstore, "root.X", [])),
        E: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.InductiveViewSet(mstore, "root.E", [a]));
        },
        r: (p: {
          first: Runtime.RowId<"root.X">,
          second: Runtime.RowId<"root.X">
        }) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.r",
            [p.first, p.second],
            [2, 3],
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