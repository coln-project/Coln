import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: runtime.MutableSet<runtime.RowId<"root.B">>,
    E: (a: runtime.RowId<"root.B">) => runtime.MutableProp,
    x: runtime.MutableRef<null>,
    next: (a: null) => runtime.MutableRef<runtime.RowId<"root.B">>,
    edge: runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (new runtime.BaseSet(mstore, "root.B", [])),
      E: (a: runtime.RowId<"root.B">) => {
        return (new runtime.BaseProp(mstore, "root.E", [a]));
      },
      x: (new runtime.ConstRef(null)),
      next: (a: null) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.next",
          [],
          [0, 1],
          {
            flatten: (a: runtime.RowId<"root.B">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.B"
              ));
            }
          }
        ));
      },
      edge: (new runtime.ConstRef(null))
    };
  }
}