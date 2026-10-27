import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableProp,
      C: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableSet<Runtime.RowId<"root.C">>,
      f: (a: Runtime.RowId<"root.A">) => (b: null) => Runtime.MutableRef<Runtime.RowId<"root.C">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseProp(mstore, "root.B", [a]));
        },
        C: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.BaseSet(mstore, "root.C", [a]));
          };
        },
        f: (a: Runtime.RowId<"root.A">) => {
          return (b: null) => {
            return (new runtime.BaseRef(
              mstore,
              "root.f",
              [a],
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