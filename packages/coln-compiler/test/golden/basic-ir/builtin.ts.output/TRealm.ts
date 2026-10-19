import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      count: (a: Runtime.RowId<"root.V">) => Runtime.MutableRef<number>,
      label: (a: Runtime.RowId<"root.V">) => Runtime.MutableRef<string>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        V: (new runtime.BaseSet(mstore, "root.V", [])),
        count: (a: Runtime.RowId<"root.V">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.count",
            [a],
            [1, 2],
            {
              flatten: (a: number) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return result[0];
              }
            }
          ));
        },
        label: (a: Runtime.RowId<"root.V">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.label",
            [a],
            [1, 2],
            {
              flatten: (a: string) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return result[0];
              }
            }
          ));
        }
      };
    }
  };
}