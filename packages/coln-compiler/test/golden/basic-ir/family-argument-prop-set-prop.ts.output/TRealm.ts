import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      R: (a: null) => (b: Runtime.RowId<"root.B">) => Runtime.MutableProp
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseSet(mbouncer, "root.B", []));
        },
        R: (a: null) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseProp(mbouncer, "root.R", [b]));
          };
        }
      };
    }
  };
}