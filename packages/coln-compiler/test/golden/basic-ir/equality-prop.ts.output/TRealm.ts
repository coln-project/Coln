import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      P: Runtime.MutableProp,
      x: Runtime.Ref<null>,
      y: Runtime.Ref<null>,
      eq: Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        P: (new runtime.ViewProp(mstore, "root.P", [])),
        x: (new runtime.ConstRef(null)),
        y: (new runtime.ConstRef(null)),
        eq: (new runtime.ConstRef(null))
      };
    }
  };
}