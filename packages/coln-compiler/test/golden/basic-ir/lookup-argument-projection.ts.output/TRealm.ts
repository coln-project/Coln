import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: (a: runtime.RowId<"root.A">) => runtime.MutableSet<runtime.RowId<"root.B">>,
    E: (x: runtime.RowId<"root.A">) => (a: runtime.RowId<"root.B">) => runtime.MutableSet<runtime.RowId<"root.E">>,
    next: (x: runtime.RowId<"root.A">) => runtime.MutableRef<runtime.RowId<"root.B">>,
    nextedge: (x: runtime.RowId<"root.A">) => runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseSet(mstore, "root.B", [a]));
      },
      E: (x: runtime.RowId<"root.A">) => {
        return (a: runtime.RowId<"root.B">) => {
          return (new runtime.BaseSet(mstore, "root.E", [x, a]));
        };
      },
      next: (x: runtime.RowId<"root.A">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.next",
          [x],
          [1, 2],
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
      nextedge: (x: runtime.RowId<"root.A">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.nextedge",
          [x],
          [1, 2],
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