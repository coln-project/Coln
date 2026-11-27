import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableProp,
      x: Runtime.Ref<null>,
      next: (a: null) => Runtime.Ref<null>,
      edge: Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.ViewProp(mstore, "root.A", [])),
        B: (new runtime.ViewProp(mstore, "root.B", [])),
        E: (a: null) => {
          return (new runtime.ViewProp(mstore, "root.E", []));
        },
        x: (new runtime.ConstRef(null)),
        next: (a: null) => {
          return (new runtime.ConstRef(null));
        },
        edge: (new runtime.ConstRef(null))
      };
    }
  };
}