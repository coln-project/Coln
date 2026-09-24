import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    Y: runtime.MutableProp,
    next: (a: runtime.RowId<"root.X">) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      Y: (new runtime.BaseProp(mstore, "root.Y", [])),
      next: (a: runtime.RowId<"root.X">) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}