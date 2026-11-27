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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        Key: (new runtime.BaseSet(mbouncer, "root.Key", [])),
        f: (a: Runtime.RowId<"root.Key">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.f",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.Key">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.Key"
                ));
              }
            }
          ));
        },
        boxed: (a: { key: Runtime.RowId<"root.Key">, value: string }) => {
          return (new runtime.BaseSet(
            mbouncer,
            "root.boxed",
            [a.key, a.value]
          ));
        }
      };
    }
  };
}