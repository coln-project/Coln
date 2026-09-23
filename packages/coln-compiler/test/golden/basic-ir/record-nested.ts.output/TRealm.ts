import * as runtime from "@coln-project/interface";

export class TRealm {
  root: runtime.MutableRef<{
    payload: { name: string, inner: { rank: number } }
  }>;

  constructor(mstore: runtime.ManagedStore) {
    this.root = (new runtime.BaseTableRef(
      mstore,
      "root",
      [],
      [0, 1, 2],
      {
        flatten: (a: {
          payload: { name: string, inner: { rank: number } }
        }) => {
          return [a.payload.name, a.payload.inner.rank];
        },
        reconstruct: (result: runtime.WireTuple) => {
          return { payload: { name: result[0], inner: { rank: result[1] } } };
        }
      }
    ));
  }
}