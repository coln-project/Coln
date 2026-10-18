import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{ count: number, label: string }>;

    constructor(mstore: Runtime.ManagedStore) {
      this.root = (new runtime.BaseTableRef(
        mstore,
        "root",
        [],
        [0, 1, 2],
        {
          flatten: (a: { count: number, label: string }) => {
            return [a.count, a.label];
          },
          reconstruct: (result: Runtime.WireTuple) => {
            return { count: result[0], label: result[1] };
          }
        }
      ));
    }
  };
}