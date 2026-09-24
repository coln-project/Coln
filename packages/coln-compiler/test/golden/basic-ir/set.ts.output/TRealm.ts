import * as runtime from "@coln-project/interface";

export class TRealm {
  root: { V: runtime.MutableSet<runtime.RowId<"root.V">> };

  constructor(mstore: runtime.ManagedStore) {
    this.root = { V: (new runtime.BaseSet(mstore, "root.V", [])) };
  }
}