import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    boxed: (a: {
      value: runtime.RowId<"root.X">
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      boxed: (a: { value: runtime.RowId<"root.X"> }) => {
        return (new runtime.BaseSet(mstore, "root.boxed", [a.value]));
      }
    };
  }
}