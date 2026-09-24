import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    A: runtime.MutableSet<runtime.RowId<"root.A">>,
    B: runtime.MutableSet<runtime.RowId<"root.B">>,
    C: runtime.MutableSet<runtime.RowId<"root.C">>,
    E: (a: runtime.RowId<"root.C">) => runtime.MutableSet<runtime.RowId<"root.E">>,
    first: (a: runtime.RowId<"root.A">) => runtime.MutableRef<runtime.RowId<"root.B">>,
    second: (a: runtime.RowId<"root.B">) => runtime.MutableRef<runtime.RowId<"root.C">>,
    edge: (x: runtime.RowId<"root.A">) => runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      A: (new runtime.BaseSet(mstore, "root.A", [])),
      B: (new runtime.BaseSet(mstore, "root.B", [])),
      C: (new runtime.BaseSet(mstore, "root.C", [])),
      E: (a: runtime.RowId<"root.C">) => {
        return (new runtime.BaseSet(mstore, "root.E", [a]));
      },
      first: (a: runtime.RowId<"root.A">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.first",
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
      second: (a: runtime.RowId<"root.B">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.second",
          [a],
          [1, 2],
          {
            flatten: (a: runtime.RowId<"root.C">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.C"
              ));
            }
          }
        ));
      },
      edge: (x: runtime.RowId<"root.A">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.edge",
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