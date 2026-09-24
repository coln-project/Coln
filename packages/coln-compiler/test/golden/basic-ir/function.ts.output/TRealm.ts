import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    Y: runtime.MutableSet<runtime.RowId<"root.Y">>,
    next: (a: runtime.RowId<"root.X">) => runtime.MutableRef<runtime.RowId<"root.Y">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      Y: (new runtime.BaseSet(mstore, "root.Y", [])),
      next: (a: runtime.RowId<"root.X">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.next",
          [a],
          [1, 2],
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