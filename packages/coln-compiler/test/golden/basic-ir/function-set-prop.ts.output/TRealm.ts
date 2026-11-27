import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Y: Runtime.MutableProp,
      next: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        Y: (new runtime.BaseProp(mbouncer, "root.Y", [])),
        next: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}