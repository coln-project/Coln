import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      key: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<{ rank: number }>,
      PayloadAt: (a: number) => Runtime.MutableSet<Runtime.RowId<"root.PayloadAt">>,
      slot: (x: Runtime.RowId<"root.X">) => Runtime.MutableRef<Runtime.RowId<"root.PayloadAt">>,
      payload: (rank: number) => (a: Runtime.RowId<"root.PayloadAt">) => Runtime.MutableRef<{
        name: string
      }>,
      E: (a: string) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      edge: (x: Runtime.RowId<"root.X">) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        key: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.key",
            [a],
            [1, 2],
            {
              flatten: (a: { rank: number }) => {
                return [a.rank];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return { rank: result[0] };
              }
            }
          ));
        },
        PayloadAt: (a: number) => {
          return (new runtime.BaseSet(mbouncer, "root.PayloadAt", [a]));
        },
        slot: (x: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.slot",
            [x],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.PayloadAt">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.PayloadAt"
                ));
              }
            }
          ));
        },
        payload: (rank: number) => {
          return (a: Runtime.RowId<"root.PayloadAt">) => {
            return (new runtime.BaseTableRef(
              mbouncer,
              "root.payload",
              [rank, a],
              [2, 3],
              {
                flatten: (a: { name: string }) => {
                  return [a.name];
                },
                reconstruct: (result: Runtime.PublicTuple) => {
                  return { name: result[0] };
                }
              }
            ));
          };
        },
        E: (a: string) => {
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