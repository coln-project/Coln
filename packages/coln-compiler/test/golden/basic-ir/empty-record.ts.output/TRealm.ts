import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{ unit: {} }>;

    constructor(mstore: Runtime.ManagedStore) {
      this.root = (new runtime.BaseRef(
        mstore,
        "root",
        [],
        [0],
        {
          flatten: (a: { unit: {} }) => {
            return [];
          },
          reconstruct: (result: Runtime.WireTuple) => {
            return { unit: {} };
          }
        }
      ));
    }
  };
}