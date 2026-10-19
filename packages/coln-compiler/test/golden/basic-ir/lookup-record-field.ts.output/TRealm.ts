import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      payload: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<{
        name: string,
        rank: number
      }>,
      E: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      edge: (x: Runtime.RowId<"root.X">) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        payload: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.payload",
            [a],
            [1, 2, 3],
            {
              flatten: (a: { name: string, rank: number }) => {
                return [a.name, a.rank];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return { name: result[0], rank: result[1] };
              }
            }
          ));
        },
        E: (a: number) => {
          return (new runtime.BaseSet(mstore, "root.E", [a]));
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