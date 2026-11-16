import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      Key: Runtime.MutableSet<Runtime.RowId<"root.Key">>,
      f: (a: Runtime.RowId<"root.Key">) => Runtime.MutableRef<Runtime.RowId<"root.Key">>,
      boxed: (a: {
        key: Runtime.RowId<"root.Key">,
        value: string
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        Key: (new runtime.BaseSet(mstore, "root.Key", [])),
        f: (a: Runtime.RowId<"root.Key">) => {
          return (new runtime.BaseRef(
            mstore,
            "root.f",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.Key">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
                  "root.Key"
                ));
              }
            }
          ));
        },
        boxed: (a: { key: Runtime.RowId<"root.Key">, value: string }) => {
          return (new runtime.BaseSet(mstore, "root.boxed", [a.key, a.value]));
        }
      };
    }
  };
}