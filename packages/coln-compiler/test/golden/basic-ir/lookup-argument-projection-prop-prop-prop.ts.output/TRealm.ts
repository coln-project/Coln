import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      E: (x: null) => (a: null) => Runtime.MutableProp,
      next: (x: null) => Runtime.MutableRef<null>,
      nextedge: (x: null) => Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
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
  };
}