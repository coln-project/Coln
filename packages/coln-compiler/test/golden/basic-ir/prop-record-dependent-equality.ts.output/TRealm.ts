import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      P: Runtime.MutableProp,
      witness: Runtime.MutableRef<{ first: null, second: null, same: null }>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        P: (new runtime.BaseProp(mbouncer, "root.P", [])),
        witness: (new runtime.BaseTableRef(
          mbouncer,
          "root.witness",
          [],
          [0],
          {
            flatten: (a: { first: null, second: null, same: null }) => {
              return [];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return { first: null, second: null, same: null };
            }
          }
        ))
      };
    }
  };
}