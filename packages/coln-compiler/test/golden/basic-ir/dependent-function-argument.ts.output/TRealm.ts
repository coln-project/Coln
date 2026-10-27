import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: (a: Runtime.RowId<"root.A">) => Runtime.MutableSet<Runtime.RowId<"root.B">>,
      C: (a: Runtime.RowId<"root.A">) => (b: Runtime.RowId<"root.B">) => Runtime.MutableSet<Runtime.RowId<"root.C">>,
      f: (a: Runtime.RowId<"root.A">) => (b: Runtime.RowId<"root.B">) => Runtime.MutableRef<Runtime.RowId<"root.C">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseSet(mstore, "root.B", [a]));
        },
        C: (a: Runtime.RowId<"root.A">) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseSet(mstore, "root.C", [a, b]));
          };
        },
        f: (a: Runtime.RowId<"root.A">) => {
          return (b: Runtime.RowId<"root.B">) => {
            return (new runtime.BaseRef(
              mstore,
              "root.f",
              [a, b],
              [2, 3],
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