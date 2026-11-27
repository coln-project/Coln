import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableProp,
      Y: Runtime.MutableSet<Runtime.RowId<"root.Y">>,
      next: (a: null) => Runtime.MutableRef<Runtime.RowId<"root.Y">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseProp(mbouncer, "root.X", [])),
        Y: (new runtime.BaseSet(mbouncer, "root.Y", [])),
        next: (a: null) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.next",
            [],
            [0, 1],
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