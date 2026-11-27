import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      rank: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<number>,
      E: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      edge: (x: Runtime.RowId<"root.X">) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        rank: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.rank",
            [a],
            [1, 2],
            {
              flatten: (a: number) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return result[0];
              }
            }
          ));
        },
        E: (a: number) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a]));
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