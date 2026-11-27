import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (a: Runtime.RowId<"root.B">) => Runtime.MutableProp,
      x: Runtime.MutableRef<null>,
      next: (a: null) => Runtime.MutableRef<Runtime.RowId<"root.B">>,
      edge: Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (new runtime.BaseSet(mbouncer, "root.B", [])),
        E: (a: Runtime.RowId<"root.B">) => {
          return (new runtime.BaseProp(mbouncer, "root.E", [a]));
        },
        x: (new runtime.ConstRef(null)),
        next: (a: null) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.next",
            [],
            [0, 1],
            {
              flatten: (a: Runtime.RowId<"root.B">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.B"
                ));
              }
            }
          ));
        },
        edge: (new runtime.ConstRef(null))
      };
    }
  };
}