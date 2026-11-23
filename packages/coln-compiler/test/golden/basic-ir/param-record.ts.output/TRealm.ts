import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      boxed: (a: {
        value: Runtime.RowId<"root.X">
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.InductiveViewSet(mstore, "root.X", [])),
        boxed: (a: { value: Runtime.RowId<"root.X"> }) => {
          return (new runtime.InductiveViewSet(
            mstore,
            "root.boxed",
            [a.value]
          ));
        }
      };
    }
  };
}