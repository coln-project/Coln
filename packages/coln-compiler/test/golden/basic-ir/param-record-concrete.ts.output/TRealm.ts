import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      boxed: (a: {
        value: { name: string }
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        boxed: (a: { value: { name: string } }) => {
          return (new runtime.BaseSet(mbouncer, "root.boxed", [a.value.name]));
        }
      };
    }
  };
}