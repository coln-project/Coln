import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        V: (new runtime.BaseProp(mbouncer, "root.V", [])),
        E: (a: null) => {
          return (new runtime.BaseSet(mbouncer, "root.E", []));
        }
      };
    }
  };
}