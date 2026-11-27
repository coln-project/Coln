import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      V: Runtime.MutableSet<Runtime.RowId<"root.V">>,
      count: (a: Runtime.RowId<"root.V">) => Runtime.MutableRef<number>,
      label: (a: Runtime.RowId<"root.V">) => Runtime.MutableRef<string>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        V: (new runtime.BaseSet(mbouncer, "root.V", [])),
        count: (a: Runtime.RowId<"root.V">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.count",
            [a],
            [1, 2],
            {
              flatten: (a: number) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return result[0];
              }
            }
          ));
        },
        label: (a: Runtime.RowId<"root.V">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.label",
            [a],
            [1, 2],
            {
              flatten: (a: string) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return result[0];
              }
            }
          ));
        }
      };
    }
  };
}