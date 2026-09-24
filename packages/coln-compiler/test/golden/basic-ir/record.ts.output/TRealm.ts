import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    point: runtime.MutableSet<runtime.RowId<"root.point">>,
    payload: (a: runtime.RowId<"root.point">) => runtime.MutableRef<{
      name: string,
      rank: number
    }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      point: (new runtime.BaseSet(mstore, "root.point", [])),
      payload: (a: runtime.RowId<"root.point">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.payload",
          [a],
          [1, 2, 3],
          {
            flatten: (a: { name: string, rank: number }) => {
              return [a.name, a.rank];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return { name: result[0], rank: result[1] };
            }
          }
        ));
      }
    };
  }
}