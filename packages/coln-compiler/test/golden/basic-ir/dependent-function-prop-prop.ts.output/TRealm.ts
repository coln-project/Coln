import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      f: (a: null) => Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (a: null) => {
          return (new runtime.BaseProp(mbouncer, "root.B", []));
        },
        f: (a: null) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}