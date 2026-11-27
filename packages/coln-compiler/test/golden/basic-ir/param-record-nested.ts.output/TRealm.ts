import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      nested: (a: {
        inner: { value: Runtime.RowId<"root.X"> }
      }) => Runtime.MutableSet<Runtime.RowId<"root.nested">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        nested: (a: { inner: { value: Runtime.RowId<"root.X"> } }) => {
          return (new runtime.BaseSet(
            mbouncer,
            "root.nested",
            [a.inner.value]
          ));
        }
      };
    }
  };
}