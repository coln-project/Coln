import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    P: runtime.MutableProp,
    evidence: runtime.MutableRef<{ proof: null }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      P: (new runtime.BaseProp(mstore, "root.P", [])),
      evidence: (new runtime.BaseTableRef(
        mstore,
        "root.evidence",
        [],
        [0],
        {
          flatten: (a: { proof: null }) => {
            return [];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return { proof: null };
          }
        }
      ))
    };
  }
}