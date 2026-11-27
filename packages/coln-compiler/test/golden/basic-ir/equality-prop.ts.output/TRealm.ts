import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      P: Runtime.MutableProp,
      x: Runtime.MutableRef<null>,
      y: Runtime.MutableRef<null>,
      eq: Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        P: (new runtime.BaseProp(mbouncer, "root.P", [])),
        x: (new runtime.ConstRef(null)),
        y: (new runtime.ConstRef(null)),
        eq: (new runtime.ConstRef(null))
      };
    }
  };
}