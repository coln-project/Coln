import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: runtime.MutableSet<runtime.RowId<"root.B">>,
    shadowed: (a: {
      value: runtime.RowId<"root.B">
    }) => runtime.MutableSet<runtime.RowId<"root.shadowed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (new runtime.BaseSet(mstore, "root.B", [])),
      shadowed: (a: { value: runtime.RowId<"root.B"> }) => {
        return (new runtime.BaseSet(mstore, "root.shadowed", [a.value]));
      }
    };
  }
}