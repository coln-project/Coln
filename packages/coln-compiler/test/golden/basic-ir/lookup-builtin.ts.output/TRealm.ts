import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      rank: (a: Runtime.RowId<"root.X">) => Runtime.Ref<number>,
      E: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      edge: (x: Runtime.RowId<"root.X">) => Runtime.Ref<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.InductiveViewSet(mstore, "root.X", [])),
        rank: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.rank",
            [a],
            [1, 2],
            {
              flatten: (a: number) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return result[0];
              }
            }
          ));
        },
        E: (a: number) => {
          return (new runtime.InductiveViewSet(mstore, "root.E", [a]));
        },
        edge: (x: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.edge",
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