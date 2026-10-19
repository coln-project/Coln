import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      C: (a: null) => (b: null) => Runtime.MutableProp,
      f: (a: null) => (b: null) => Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
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
  };
}