import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{}>;

    constructor(mstore: Runtime.ManagedStore) {
      this.root = (new runtime.BaseTableRef(
        mstore,
        "root",
        [],
        [0],
        {
          flatten: (a: {}) => {
            return [];
          },
          reconstruct: (result: Runtime.WireTuple) => {
            return {};
          }
        }
      ));
    }
  };
}