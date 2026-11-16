import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      E: (x: Runtime.RowId<"root.A">) => (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      next: (x: Runtime.RowId<"root.A">) => Runtime.MutableRef<null>,
      nextedge: (x: Runtime.RowId<"root.A">) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseProp(mstore, "root.B", [a]));
        },
        E: (x: Runtime.RowId<"root.A">) => {
          return (a: null) => {
            return (new runtime.BaseSet(mstore, "root.E", [x]));
          };
        },
        next: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.ConstRef(null));
        },
        nextedge: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseRef(
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