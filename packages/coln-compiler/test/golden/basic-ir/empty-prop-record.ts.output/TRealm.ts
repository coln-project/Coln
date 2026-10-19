import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{ truth: {} }>;

    constructor(mstore: Runtime.ManagedStore) {
      this.root = (new runtime.BaseTableRef(
        mstore,
        "root",
        [],
        [0],
        {
          flatten: (a: { truth: {} }) => {
            return [];
          },
          reconstruct: (result: Runtime.WireTuple) => {
            return { truth: {} };
          }
        }
      ));
    }
  };
}