import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: Runtime.MutableSet<Runtime.RowId<"root.B">>,
      shadowed: (a: {
        value: Runtime.RowId<"root.B">
      }) => Runtime.MutableSet<Runtime.RowId<"root.shadowed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.BaseSet(mstore, "root.A", [])),
        B: (new runtime.BaseSet(mstore, "root.B", [])),
        shadowed: (a: { value: Runtime.RowId<"root.B"> }) => {
          return (new runtime.BaseSet(mstore, "root.shadowed", [a.value]));
        }
      };
    }
  };
}