import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: { P: Runtime.MutableProp, evidence: Runtime.Ref<{ proof: null }> };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        P: (new runtime.ViewProp(mstore, "root.P", [])),
        evidence: (new runtime.BaseTableRef(
          mstore,
          "root.evidence",
          [],
          [0],
          {
            flatten: (a: { proof: null }) => {
              return [];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return { proof: null };
            }
          }
        ))
      };
    }
  };
}