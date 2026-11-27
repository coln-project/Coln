import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      P: (a: number) => Runtime.MutableProp,
      package: Runtime.MutableRef<{ value: number, evidence: { proof: null } }>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        P: (a: number) => {
          return (new runtime.BaseProp(mbouncer, "root.P", [a]));
        },
        package: (new runtime.BaseTableRef(
          mbouncer,
          "root.package",
          [],
          [0, 1],
          {
            flatten: (a: { value: number, evidence: { proof: null } }) => {
              return [a.value];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return { value: result[0], evidence: { proof: null } };
            }
          }
        ))
      };
    }
  };
}