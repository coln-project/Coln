import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      R: (a: null) => (b: null) => Runtime.MutableSet<Runtime.RowId<"root.R">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseProp(mbouncer, "root.B", []));
        },
        R: (a: null) => {
          return (b: null) => {
            return (new runtime.BaseSet(mbouncer, "root.R", []));
          };
        }
      };
    }
  };
}