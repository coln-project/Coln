import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      Y: Runtime.MutableSet<Runtime.RowId<"root.Y">>,
      next: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<Runtime.RowId<"root.Y">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        Y: (new runtime.BaseSet(mstore, "root.Y", [])),
        next: (a: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseRef(
            mstore,
            "root.next",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.Y">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.Y"
                ));
              }
            }
          ));
        }
      };
    }
  };
}