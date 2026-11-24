import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (x: null) => (a: Runtime.RowId<"root.B">) => Runtime.MutableProp,
      next: (x: null) => Runtime.MutableRef<Runtime.RowId<"root.B">>,
      nextedge: (x: null) => Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseSet(mbouncer, "root.B", []));
        },
        E: (x: null) => {
          return (a: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseProp(mbouncer, "root.E", [a]));
          };
        },
        next: (x: null) => {
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
        nextedge: (x: null) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}