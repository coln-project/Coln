import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      P: Runtime.MutableProp,
      Q: Runtime.MutableProp,
      R: (a: Runtime.RowId<"root.X">) => (b: null) => (c: null) => Runtime.MutableProp
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        P: (new runtime.BaseProp(mbouncer, "root.P", [])),
        Q: (new runtime.BaseProp(mbouncer, "root.Q", [])),
        R: (a: Runtime.RowId<"root.X">) => {
          return (b: null) => {
            return (c: null) => {
              return (new runtime.BaseProp(mbouncer, "root.R", [a]));
            };
          };
        }
      };
    }
  };
}