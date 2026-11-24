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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        point: (new runtime.BaseSet(mbouncer, "root.point", [])),
        payload: (a: Runtime.RowId<"root.point">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.payload",
            [a],
            [1, 2, 3],
            {
              flatten: (a: { name: string, rank: number }) => {
                return [a.name, a.rank];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return { name: result[0], rank: result[1] };
              }
            }
          ));
        }
      };
    }
  };
}