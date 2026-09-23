import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableSet<runtime.RowId<"root.B">>,
    E: (x: null) => (a: runtime.RowId<"root.B">) => runtime.MutableProp,
    next: (x: null) => runtime.MutableRef<runtime.RowId<"root.B">>,
    nextedge: (x: null) => runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseSet(mstore, "root.B", []));
      },
      E: (x: null) => {
        return (a: runtime.RowId<"root.B">) => {
          return (new runtime.BaseProp(mstore, "root.E", [a]));
        };
      },
      next: (x: null) => {
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
      nextedge: (x: null) => {
        return (new runtime.ConstRef(null));
      }
    };
  }
}