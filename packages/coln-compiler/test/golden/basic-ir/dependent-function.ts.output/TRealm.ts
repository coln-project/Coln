import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      f: (a: Runtime.RowId<"root.A">) => Runtime.MutableRef<Runtime.RowId<"root.B">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseSet(mstore, "root.B", [a]));
        },
        f: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.f",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.B">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.B"
                ));
              }
            }
          ));
        }
      };
    }
  };
}