import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      C: (a: null) => (b: Runtime.RowId<"root.B">) => Runtime.MutableSet<Runtime.RowId<"root.C">>,
      f: (a: null) => (b: Runtime.RowId<"root.B">) => Runtime.MutableRef<Runtime.RowId<"root.C">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseProp(mstore, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseSet(mstore, "root.B", []));
        },
        C: (a: null) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseSet(mstore, "root.C", [b]));
          };
        },
        f: (a: null) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseRef(
              mstore,
              "root.f",
              [b],
              [1, 2],
              {
                flatten: (a: Runtime.RowId<"root.C">) => {
                  return [a];
                },
                reconstruct: (result: Runtime.WireTuple) => {
                  return (new runtime.RowId(
                    { type: "Existing", value: result[0] as Runtime.WireRowId },
                    "root.C"
                  ));
                }
              }
            ));
          };
        }
      };
    }
  };
}