import * as runtime from "@coln-project/interface";

export class TRealm {
  root: runtime.MutableRef<{ unit: {} }>;

  constructor(mstore: runtime.ManagedStore) {
    this.root = (new runtime.BaseTableRef(
      mstore,
      "root",
      [],
      [0],
      {
        flatten: (a: { unit: {} }) => {
          return [];
        },
        reconstruct: (result: runtime.WireTuple) => {
          return { unit: {} };
        }
      }
    ));
  }
}