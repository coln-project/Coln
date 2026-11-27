import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: Runtime.MutableProp,
      E: (a: null) => Runtime.MutableProp,
      x: Runtime.MutableRef<null>,
      next: (a: null) => Runtime.MutableRef<null>,
      edge: Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseProp(mbouncer, "root.A", [])),
        B: (new runtime.BaseProp(mbouncer, "root.B", [])),
        E: (a: null) => {
          return (new runtime.BaseProp(mbouncer, "root.E", []));
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