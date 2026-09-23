import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    V: runtime.MutableSet<runtime.RowId<"root.V">>,
    E: (a: runtime.RowId<"root.V">) => runtime.MutableProp
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      V: (new runtime.BaseSet(mstore, "root.V", [])),
      E: (a: runtime.RowId<"root.V">) => {
        return (new runtime.BaseProp(mstore, "root.E", [a]));
      }
    };
  }
}