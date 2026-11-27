import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      C: (a: null) => (b: Runtime.RowId<"root.B">) => Runtime.MutableSet<Runtime.RowId<"root.C">>,
      f: (a: null) => (b: Runtime.RowId<"root.B">) => Runtime.MutableRef<Runtime.RowId<"root.C">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseSet(mbouncer, "root.B", []));
        },
        C: (a: null) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseSet(mbouncer, "root.C", [b]));
          };
        },
        f: (a: null) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseTableRef(
              mbouncer,
              "root.f",
              [b],
              [1, 2],
              {
                flatten: (a: Runtime.RowId<"root.C">) => {
                  return [a];
                },
                reconstruct: (result: Runtime.PublicTuple) => {
                  return (new runtime.RowId(
                    {
                      type: "Existing",
                      value: result[0] as Runtime.PublicRowId
                    },
                    "root.C"
                  ));
                }
              }
            ));
          };
        }
      };
    }
  };
}