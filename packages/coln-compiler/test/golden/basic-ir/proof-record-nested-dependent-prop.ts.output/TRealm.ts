import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      P: (a: number) => Runtime.MutableProp,
      package: Runtime.MutableRef<{ value: number, evidence: { proof: null } }>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        P: (a: number) => {
          return (new runtime.BaseProp(mstore, "root.P", [a]));
        },
        package: (new runtime.BaseRef(
          mstore,
          "root.package",
          [],
          [0, 1],
          {
            flatten: (a: { value: number, evidence: { proof: null } }) => {
              return [a.value];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return { value: result[0] as number, evidence: { proof: null } };
            }
          }
        ))
      };
    }
  };
}