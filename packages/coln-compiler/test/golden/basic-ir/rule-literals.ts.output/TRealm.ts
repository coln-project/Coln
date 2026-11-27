import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      count: (a: Runtime.RowId<"root.X">) => Runtime.Ref<number>,
      label: (a: Runtime.RowId<"root.X">) => Runtime.Ref<string>,
      countIs19: (x: Runtime.RowId<"root.X">) => Runtime.Ref<null>,
      labelIsZombocom: (x: Runtime.RowId<"root.X">) => Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.InductiveViewSet(mstore, "root.X", [])),
        count: (a: Runtime.RowId<"root.X">) => {
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
        label: (a: Runtime.RowId<"root.X">) => {
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