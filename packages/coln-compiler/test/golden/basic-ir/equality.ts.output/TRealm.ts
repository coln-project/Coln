import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      x: Runtime.MutableRef<Runtime.RowId<"root.V">>,
      y: Runtime.MutableRef<Runtime.RowId<"root.V">>,
      eq: Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        V: (new runtime.BaseSet(mstore, "root.V", [])),
        x: (new runtime.BaseRef(
          mstore,
          "root.x",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.V">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.V"
              ));
            }
          }
        )),
        y: (new runtime.BaseRef(
          mstore,
          "root.y",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.V">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.V"
              ));
            }
          }
        )),
        eq: (new runtime.ConstRef(null))
      };
    }
  };
}