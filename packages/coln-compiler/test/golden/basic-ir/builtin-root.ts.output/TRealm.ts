import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{ count: number, label: string }>;

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = (new runtime.BaseTableRef(
        mbouncer,
        "root",
        [],
        [0, 1, 2],
        {
          flatten: (a: { count: number, label: string }) => {
            return [a.count, a.label];
          },
          reconstruct: (result: Runtime.PublicTuple) => {
            return { count: result[0], label: result[1] };
          }
        }
      ));
    }
  };
}