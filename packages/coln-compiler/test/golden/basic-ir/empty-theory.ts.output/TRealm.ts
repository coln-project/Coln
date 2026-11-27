import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{}>;

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = (new runtime.BaseTableRef(
        mbouncer,
        "root",
        [],
        [0],
        {
          flatten: (a: {}) => {
            return [];
          },
          reconstruct: (result: Runtime.PublicTuple) => {
            return {};
          }
        }
      ));
    }
  };
}