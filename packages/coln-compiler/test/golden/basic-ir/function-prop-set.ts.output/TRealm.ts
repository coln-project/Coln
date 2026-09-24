import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableProp,
    Y: runtime.MutableSet<runtime.RowId<"root.Y">>,
    next: (a: null) => runtime.MutableRef<runtime.RowId<"root.Y">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseProp(mstore, "root.X", [])),
      Y: (new runtime.BaseSet(mstore, "root.Y", [])),
      next: (a: null) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.next",
          [],
          [0, 1],
          {
            flatten: (a: runtime.RowId<"root.Y">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.Y"
              ));
            }
          }
        ));
      }
    };
  }
}