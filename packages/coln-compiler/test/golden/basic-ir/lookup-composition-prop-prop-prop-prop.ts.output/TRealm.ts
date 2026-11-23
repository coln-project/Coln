import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableProp,
      C: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableProp,
      first: (a: null) => Runtime.Ref<null>,
      second: (a: null) => Runtime.Ref<null>,
      edge: (x: null) => Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.ViewProp(mstore, "root.A", [])),
        B: (new runtime.ViewProp(mstore, "root.B", [])),
        C: (new runtime.ViewProp(mstore, "root.C", [])),
        E: (a: null) => {
          return (new runtime.ViewProp(mstore, "root.E", []));
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