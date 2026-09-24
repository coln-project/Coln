import * as runtime from "@coln-project/interface";

export class TRealm {
  root: runtime.MutableRef<{ truth: {} }>;

  constructor(mstore: runtime.ManagedStore) {
    this.root = (new runtime.BaseTableRef(
      mstore,
      "root",
      [],
      [0],
      {
        flatten: (a: { truth: {} }) => {
          return [];
        },
        reconstruct: (result: runtime.WireTuple) => {
          return { truth: {} };
        }
      }
    ));
  }
}