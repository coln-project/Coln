import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: { V: Runtime.MutableProp, E: (a: null) => Runtime.MutableProp };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        V: (new runtime.ViewProp(mstore, "root.V", [])),
        E: (a: null) => {
          return (new runtime.ViewProp(mstore, "root.E", []));
        }
      };
    }
  };
}