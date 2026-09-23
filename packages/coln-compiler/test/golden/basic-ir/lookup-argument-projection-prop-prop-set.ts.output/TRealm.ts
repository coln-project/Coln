import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableProp,
    B: (a: null) => runtime.MutableProp,
    E: (x: null) => (a: null) => runtime.MutableSet<runtime.RowId<"root.E">>,
    next: (x: null) => runtime.MutableRef<null>,
    nextedge: (x: null) => runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseProp(mstore, "root.A", [])),
      B: (a: null) => {
        return (new runtime.BaseProp(mstore, "root.B", []));
      },
      E: (x: null) => {
        return (a: null) => {
          return (new runtime.BaseSet(mstore, "root.E", []));
        };
      },
      next: (x: null) => {
        return (new runtime.ConstRef(null));
      },
      nextedge: (x: null) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.nextedge",
          [],
          [0, 1],
          {
            flatten: (a: runtime.RowId<"root.E">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.E"
              ));
            }
          }
        ));
      }
    };
  }
}