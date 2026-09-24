import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: runtime.MutableProp,
    C: runtime.MutableProp,
    E: (a: null) => runtime.MutableProp,
    first: (a: null) => runtime.MutableRef<null>,
    second: (a: null) => runtime.MutableRef<null>,
    edge: (x: null) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (new runtime.BaseProp(mstore, "root.B", [])),
      C: (new runtime.BaseProp(mstore, "root.C", [])),
      E: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.E", []));
      },
      first: (a: null) => {
        return (new runtime.ConstRef(null));
      },
      second: (a: null) => {
        return (new runtime.ConstRef(null));
      },
      edge: (x: null) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}