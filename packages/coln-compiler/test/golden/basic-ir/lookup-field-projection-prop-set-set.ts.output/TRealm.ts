import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableSet<Runtime.RowId<"root.B">>,
      E: (a: Runtime.RowId<"root.B">) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      x: Runtime.MutableRef<null>,
      next: (a: null) => Runtime.MutableRef<Runtime.RowId<"root.B">>,
      edge: Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseProp(mstore, "root.A", [])),
        B: (new runtime.BaseSet(mstore, "root.B", [])),
        E: (a: Runtime.RowId<"root.B">) => {
          return (new runtime.BaseSet(mstore, "root.E", [a]));
        },
        x: (new runtime.ConstRef(null)),
        next: (a: null) => {
          return (new runtime.BaseRef(
            mstore,
            "root.next",
            [],
            [0, 1],
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
        edge: (new runtime.BaseRef(
          mstore,
          "root.edge",
          [],
          [0, 1],
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
        ))
      };
    }
  };
}