import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (a: Runtime.RowId<"root.B">) => Runtime.MutableProp,
      x: Runtime.MutableRef<Runtime.RowId<"root.A">>,
      next: (a: Runtime.RowId<"root.A">) => Runtime.MutableRef<Runtime.RowId<"root.B">>,
      edge: Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (new runtime.BaseSet(mstore, "root.B", [])),
        E: (a: Runtime.RowId<"root.B">) => {
          return (new runtime.BaseProp(mstore, "root.E", [a]));
        },
        x: (new runtime.BaseRef(
          mstore,
          "root.x",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.A">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.A"
              ));
            }
          }
        )),
        next: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseRef(
            mstore,
            "root.next",
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
        },
        edge: (new runtime.ConstRef(null))
      };
    }
  };
}