import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Y: Runtime.MutableSet<Runtime.RowId<"root.Y">>,
      next: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<Runtime.RowId<"root.Y">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        Y: (new runtime.BaseSet(mbouncer, "root.Y", [])),
        next: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.next",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.Y">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
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