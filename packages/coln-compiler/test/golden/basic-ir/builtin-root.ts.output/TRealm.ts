import * as runtime from "@coln-project/interface";

export class TRealm {
  root: runtime.MutableRef<{ count: number, label: string }>;

  constructor(mstore: runtime.ManagedStore) {
    this.root = (new runtime.BaseTableRef(
      mstore,
      "root",
      [],
      [0, 1, 2],
      {
        flatten: (a: { count: number, label: string }) => {
          return [a.count, a.label];
        },
        reconstruct: (result: runtime.WireTuple) => {
          return { count: result[0], label: result[1] };
        }
      }
    ));
  }
}