import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    P: runtime.MutableProp,
    witness: runtime.MutableRef<{ first: null, second: null, same: null }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      P: (new runtime.BaseProp(mstore, "root.P", [])),
      witness: (new runtime.BaseTableRef(
        mstore,
        "root.witness",
        [],
        [0],
        {
          flatten: (a: { first: null, second: null, same: null }) => {
            return [];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return { first: null, second: null, same: null };
          }
        }
      ))
    };
  }
}