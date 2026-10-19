import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      nested: (a: {
        inner: { value: Runtime.RowId<"root.X"> }
      }) => Runtime.MutableSet<Runtime.RowId<"root.nested">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        nested: (a: { inner: { value: Runtime.RowId<"root.X"> } }) => {
          return (new runtime.BaseSet(mstore, "root.nested", [a.inner.value]));
        }
      };
    }
  };
}