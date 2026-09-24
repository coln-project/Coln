import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    nested: (a: {
      inner: { value: runtime.RowId<"root.X"> }
    }) => runtime.MutableSet<runtime.RowId<"root.nested">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      nested: (a: { inner: { value: runtime.RowId<"root.X"> } }) => {
        return (new runtime.BaseSet(mstore, "root.nested", [a.inner.value]));
      }
    };
  }
}