import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      E: (x: null) => (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      next: (x: null) => Runtime.MutableRef<null>,
      nextedge: (x: null) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseProp(mbouncer, "root.B", []));
        },
        E: (x: null) => {
          return (a: null) => {
            return (new runtime.BaseSet(mbouncer, "root.E", []));
          };
        },
        next: (x: null) => {
          return (new runtime.ConstRef(null));
        },
        nextedge: (x: null) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.nextedge",
            [],
            [0, 1],
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