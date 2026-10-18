import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      point: Runtime.MutableSet<Runtime.RowId<"root.point">>,
      payload: (a: Runtime.RowId<"root.point">) => Runtime.MutableRef<{
        name: string,
        rank: number
      }>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        point: (new runtime.BaseSet(mstore, "root.point", [])),
        payload: (a: Runtime.RowId<"root.point">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.payload",
            [a],
            [1, 2, 3],
            {
              flatten: (a: { name: string, rank: number }) => {
                return [a.name, a.rank];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return { name: result[0], rank: result[1] };
              }
            }
          ));
        }
      };
    }
  };
}