import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableProp,
      C: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableProp,
      first: (a: null) => Runtime.MutableRef<null>,
      second: (a: null) => Runtime.MutableRef<null>,
      edge: (x: null) => Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
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
  };
}