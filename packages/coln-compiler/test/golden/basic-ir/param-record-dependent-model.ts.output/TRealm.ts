import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      pointed: Runtime.MutableRef<{ point: { rank: number } }>,
      boxed: (a: {
        key: { rank: number },
        value: string
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        pointed: (new runtime.BaseTableRef(
          mbouncer,
          "root.pointed",
          [],
          [0, 1],
          {
            flatten: (a: { point: { rank: number } }) => {
              return [a.point.rank];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return { point: { rank: result[0] } };
            }
          }
        )),
        boxed: (a: { key: { rank: number }, value: string }) => {
          return (new runtime.BaseSet(
            mbouncer,
            "root.boxed",
            [a.key.rank, a.value]
          ));
        }
      };
    }
  };
}