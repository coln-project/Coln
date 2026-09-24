import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    P: (a: number) => runtime.MutableProp,
    package: runtime.MutableRef<{ value: number, evidence: { proof: null } }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      P: (a: number) => {
        return (new runtime.BaseProp(mstore, "root.P", [a]));
      },
      package: (new runtime.BaseTableRef(
        mstore,
        "root.package",
        [],
        [0, 1],
        {
          flatten: (a: { value: number, evidence: { proof: null } }) => {
            return [a.value];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return { value: result[0], evidence: { proof: null } };
          }
        }
      ))
    };
  }
}