import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableProp,
      Y: Runtime.MutableProp,
      next: (a: null) => Runtime.MutableRef<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseProp(mstore, "root.X", [])),
        Y: (new runtime.BaseProp(mstore, "root.Y", [])),
        next: (a: null) => {
          return (new runtime.ConstRef(null));
        }
      };
    }
  };
}