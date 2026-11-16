import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      x: Runtime.MutableRef<null>,
      next: (a: null) => Runtime.MutableRef<null>,
      edge: Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseProp(mstore, "root.A", [])),
        B: (new runtime.BaseProp(mstore, "root.B", [])),
        E: (a: null) => {
          return (new runtime.BaseSet(mstore, "root.E", []));
        },
        x: (new runtime.ConstRef(null)),
        next: (a: null) => {
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