import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      boxed: (a: {
        value: Runtime.RowId<"root.X">
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        boxed: (a: { value: Runtime.RowId<"root.X"> }) => {
          return (new runtime.BaseSet(mbouncer, "root.boxed", [a.value]));
        }
      };
    }
  };
}