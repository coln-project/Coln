import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{ truth: {} }>;

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = (new runtime.BaseTableRef(
        mbouncer,
        "root",
        [],
        [0],
        {
          flatten: (a: { truth: {} }) => {
            return [];
          },
          reconstruct: (result: Runtime.PublicTuple) => {
            return { truth: {} };
          }
        }
      ));
    }
  };
}