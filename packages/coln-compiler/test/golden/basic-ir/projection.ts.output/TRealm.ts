import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      E: (a: Runtime.RowId<"root.X">) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      r: (p: {
        first: Runtime.RowId<"root.X">,
        second: Runtime.RowId<"root.X">
      }) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        E: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a]));
        },
        r: (p: {
          first: Runtime.RowId<"root.X">,
          second: Runtime.RowId<"root.X">
        }) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.r",
            [p.first, p.second],
            [2, 3],
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