import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      R: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableProp
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseSet(mbouncer, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseProp(mbouncer, "root.B", [a]));
        },
        R: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.BaseProp(mbouncer, "root.R", [a]));
          };
        }
      };
    }
  };
}