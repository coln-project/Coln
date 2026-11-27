import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{ unit: {} }>;

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = (new runtime.BaseTableRef(
        mbouncer,
        "root",
        [],
        [0],
        {
          flatten: (a: { unit: {} }) => {
            return [];
          },
          reconstruct: (result: Runtime.PublicTuple) => {
            return { unit: {} };
          }
        }
      ));
    }
  };
}