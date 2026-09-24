import * as runtime from "@coln-project/interface";

export class TRealm {
  root: runtime.MutableRef<{}>;

  constructor(mstore: runtime.ManagedStore) {
    this.root = (new runtime.BaseTableRef(
      mstore,
      "root",
      [],
      [0],
      {
        flatten: (a: {}) => {
          return [];
        },
        reconstruct: (result: runtime.WireTuple) => {
          return {};
        }
      }
    ));
  }
}