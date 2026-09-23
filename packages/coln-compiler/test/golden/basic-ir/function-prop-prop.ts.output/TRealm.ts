import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableProp,
    Y: runtime.MutableProp,
    next: (a: null) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseProp(mstore, "root.X", [])),
      Y: (new runtime.BaseProp(mstore, "root.Y", [])),
      next: (a: null) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}