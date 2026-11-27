import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      E: (a: Runtime.RowId<"root.V">) => Runtime.MutableSet<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        V: (new runtime.BaseSet(mbouncer, "root.V", [])),
        E: (a: Runtime.RowId<"root.V">) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a]));
        }
      };
    }
  };
}