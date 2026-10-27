import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      x: Runtime.MutableRef<Runtime.RowId<"root.A">>,
      next: (a: Runtime.RowId<"root.A">) => Runtime.MutableRef<null>,
      edge: Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (new runtime.BaseProp(mstore, "root.B", [])),
        E: (a: null) => {
          return (new runtime.BaseSet(mstore, "root.E", []));
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
          return (new runtime.ConstRef(null));
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