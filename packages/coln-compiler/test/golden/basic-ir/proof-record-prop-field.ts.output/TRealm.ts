import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      P: Runtime.MutableProp,
      evidence: Runtime.MutableRef<{ proof: null }>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        P: (new runtime.BaseProp(mbouncer, "root.P", [])),
        evidence: (new runtime.BaseTableRef(
          mbouncer,
          "root.evidence",
          [],
          [0],
          {
            flatten: (a: { proof: null }) => {
              return [];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return { proof: null };
            }
          }
        ))
      };
    }
  };
}