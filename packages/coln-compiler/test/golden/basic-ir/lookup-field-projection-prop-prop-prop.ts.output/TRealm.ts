import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: runtime.MutableProp,
    E: (a: null) => runtime.MutableProp,
    x: runtime.MutableRef<null>,
    next: (a: null) => runtime.MutableRef<null>,
    edge: runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (new runtime.BaseProp(mstore, "root.B", [])),
      E: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.E", []));
      },
      x: (new runtime.ConstRef(null)),
      next: (a: null) => {
        return (new runtime.ConstRef(null));
      },
      edge: (new runtime.ConstRef(null))
    };
  }
}