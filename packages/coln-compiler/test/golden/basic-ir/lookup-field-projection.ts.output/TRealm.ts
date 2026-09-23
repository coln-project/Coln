import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: runtime.MutableSet<runtime.RowId<"root.B">>,
    E: (a: runtime.RowId<"root.B">) => runtime.MutableSet<runtime.RowId<"root.E">>,
    x: runtime.MutableRef<runtime.RowId<"root.A">>,
    next: (a: runtime.RowId<"root.A">) => runtime.MutableRef<runtime.RowId<"root.B">>,
    edge: runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (new runtime.BaseSet(mstore, "root.B", [])),
      E: (a: runtime.RowId<"root.B">) => {
        return (new runtime.BaseSet(mstore, "root.E", [a]));
      },
      x: (new runtime.BaseTableRef(
        mstore,
        "root.x",
        [],
        [0, 1],
        {
          flatten: (a: runtime.RowId<"root.A">) => {
            return [a];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return (new runtime.RowId(
              { type: "Existing", value: result[0] as runtime.WireRowId },
              "root.A"
            ));
          }
        }
      )),
      next: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.next",
          [a],
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
      edge: (new runtime.BaseTableRef(
        mstore,
        "root.edge",
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
      ))
    };
  }
}