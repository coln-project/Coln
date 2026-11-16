import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      P: Runtime.MutableProp,
      witness: Runtime.MutableRef<{ first: null, second: null, same: null }>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        P: (new runtime.BaseProp(mstore, "root.P", [])),
        witness: (new runtime.BaseRef(
          mstore,
          "root.witness",
          [],
          [0],
          {
            flatten: (a: { first: null, second: null, same: null }) => {
              return [];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return { first: null, second: null, same: null };
            }
          }
        ))
      };
    }
  };
}