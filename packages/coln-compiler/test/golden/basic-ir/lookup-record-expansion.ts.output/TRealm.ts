import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      payload: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<{
        name: string,
        rank: number
      }>,
      E: (a: {
        name: string,
        rank: number
      }) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      edge: (x: Runtime.RowId<"root.X">) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        payload: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.payload",
            [a],
            [1, 2, 3],
            {
              flatten: (a: { name: string, rank: number }) => {
                return [a.name, a.rank];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return { name: result[0], rank: result[1] };
              }
            }
          ));
        },
        E: (a: { name: string, rank: number }) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a.name, a.rank]));
        },
        edge: (x: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.edge",
            [x],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.E">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
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