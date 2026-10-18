import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (x: Runtime.RowId<"root.A">) => (a: Runtime.RowId<"root.B">) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      next: (x: Runtime.RowId<"root.A">) => Runtime.MutableRef<Runtime.RowId<"root.B">>,
      nextedge: (x: Runtime.RowId<"root.A">) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseSet(mstore, "root.B", [a]));
        },
        E: (x: Runtime.RowId<"root.A">) => {
          return (a: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseSet(mstore, "root.E", [x, a]));
          };
        },
        next: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.next",
            [x],
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
        },
        nextedge: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.nextedge",
            [x],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.E">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.E"
                ));
              }
            }
          ));
        }
      };
    }
  };
}