import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    V: runtime.MutableSet<runtime.RowId<"root.V">>,
    E: (a: runtime.RowId<"root.V">) => runtime.MutableSet<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      V: (new runtime.BaseSet(mstore, "root.V", [])),
      E: (a: runtime.RowId<"root.V">) => {
        return (new runtime.BaseSet(mstore, "root.E", [a]));
      }
    };
  }
}