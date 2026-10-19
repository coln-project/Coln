import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      outer: { inner: Runtime.MutableRef<{ point: Runtime.RowId<"root.X"> }> }
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        outer: {
          inner: (new runtime.BaseTableRef(
            mstore,
            "root.outer.inner",
            [],
            [0, 1],
            {
              flatten: (a: { point: Runtime.RowId<"root.X"> }) => {
                return [a.point];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return {
                  point: (new runtime.RowId(
                    { type: "Existing", value: result[0] as Runtime.WireRowId },
                    "root.X"
                  ))
                };
              }
            }
          ))
        }
      };
    }
  };
}