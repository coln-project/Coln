import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      count: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<number>,
      label: (a: Runtime.RowId<"root.X">) => Runtime.MutableRef<string>,
      countIs19: (x: Runtime.RowId<"root.X">) => Runtime.MutableRef<null>,
      labelIsZombocom: (x: Runtime.RowId<"root.X">) => Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        X: (new runtime.BaseSet(mbouncer, "root.X", [])),
        count: (a: Runtime.RowId<"root.X">) => {
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
        label: (a: Runtime.RowId<"root.X">) => {
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
        },
        countIs19: (x: Runtime.RowId<"root.X">) => {
          return (new runtime.ConstRef(null));
        },
        labelIsZombocom: (x: Runtime.RowId<"root.X">) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}