import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    pointed: runtime.MutableRef<{ point: { rank: number } }>,
    boxed: (a: {
      key: { rank: number },
      value: string
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      pointed: (new runtime.BaseTableRef(
        mstore,
        "root.pointed",
        [],
        [0, 1],
        {
          flatten: (a: { point: { rank: number } }) => {
            return [a.point.rank];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return { point: { rank: result[0] } };
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
}