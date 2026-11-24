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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseSet(mbouncer, "root.A", [])),
        B: (new runtime.BaseSet(mbouncer, "root.B", [])),
        shadowed: (a: { value: Runtime.RowId<"root.B"> }) => {
          return (new runtime.BaseSet(mbouncer, "root.shadowed", [a.value]));
        }
      };
    }
  };
}