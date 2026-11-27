import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      C: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableSet<Runtime.RowId<"root.C">>,
      f: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableRef<Runtime.RowId<"root.C">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseSet(mbouncer, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseProp(mbouncer, "root.B", [a]));
        },
        C: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.BaseSet(mbouncer, "root.C", [a]));
          };
        },
        f: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.BaseTableRef(
              mbouncer,
              "root.f",
              [a],
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