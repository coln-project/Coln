import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableProp,
    E: (x: null) => (a: null) => runtime.MutableProp,
    next: (x: null) => runtime.MutableRef<null>,
    nextedge: (x: null) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.B", []));
      },
      E: (x: null) => {
        return (a: null) => {
          return (new runtime.BaseProp(mstore, "root.E", []));
        };
      },
      next: (x: null) => {
        return (new runtime.ConstRef(null));
      },
      nextedge: (x: null) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}