import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableProp,
    C: (a: null) => (b: null) => runtime.MutableProp,
    f: (a: null) => (b: null) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.B", []));
      },
      C: (a: null) => {
        return (b: null) => {
          return (new runtime.BaseProp(mstore, "root.C", []));
        };
      },
      f: (a: null) => {
        return (b: null) => {
          return (new runtime.ConstRef(null));
        };
      }
    };
  }
}