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

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        pointed: (new runtime.BaseRef(
          mstore,
          "root.pointed",
          [],
          [0, 1],
          {
            flatten: (a: { point: { rank: number } }) => {
              return [a.point.rank];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return { point: { rank: result[0] as number } };
            }
          }
        )),
        boxed: (a: { key: { rank: number }, value: string }) => {
          return (new runtime.BaseSet(
            mstore,
            "root.boxed",
            [a.key.rank, a.value]
          ));
        }
      };
    }
  };
}