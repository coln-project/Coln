import * as runtime from "@coln-project/interface";

export class TRealm {
  root: { X: runtime.MutableSet<runtime.RowId<"root.X">> };

  constructor(mstore: runtime.ManagedStore) {
    this.root = { X: (new runtime.BaseSet(mstore, "root.X", [])) };
  }
}