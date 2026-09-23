import * as runtime from "@coln-project/interface";

export class TRealm {
  root: { V: runtime.MutableProp, E: (a: null) => runtime.MutableProp };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      V: (new runtime.BaseProp(mstore, "root.V", [])),
      E: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.E", []));
      }
    };
  }
}