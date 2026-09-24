import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    V: runtime.MutableProp,
    E: (a: null) => runtime.MutableSet<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      V: (new runtime.BaseProp(mstore, "root.V", [])),
      E: (a: null) => {
        return (new runtime.BaseSet(mstore, "root.E", []));
      }
    };
  }
}