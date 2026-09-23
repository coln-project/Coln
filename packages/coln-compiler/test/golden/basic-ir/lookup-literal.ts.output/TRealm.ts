import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    IntEdge: (a: number) => runtime.MutableSet<runtime.RowId<"root.IntEdge">>,
    StringEdge: (a: string) => runtime.MutableSet<runtime.RowId<"root.StringEdge">>,
    intEdge: runtime.MutableRef<runtime.RowId<"root.IntEdge">>,
    stringEdge: runtime.MutableRef<runtime.RowId<"root.StringEdge">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      IntEdge: (a: number) => {
        return (new runtime.BaseSet(mstore, "root.IntEdge", [a]));
      },
      StringEdge: (a: string) => {
        return (new runtime.BaseSet(mstore, "root.StringEdge", [a]));
      },
      intEdge: (new runtime.BaseTableRef(
        mstore,
        "root.intEdge",
        [],
        [0, 1],
        {
          flatten: (a: runtime.RowId<"root.IntEdge">) => {
            return [a];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return (new runtime.RowId(
              { type: "Existing", value: result[0] as runtime.WireRowId },
              "root.IntEdge"
            ));
          }
        }
      )),
      stringEdge: (new runtime.BaseTableRef(
        mstore,
        "root.stringEdge",
        [],
        [0, 1],
        {
          flatten: (a: runtime.RowId<"root.StringEdge">) => {
            return [a];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return (new runtime.RowId(
              { type: "Existing", value: result[0] as runtime.WireRowId },
              "root.StringEdge"
            ));
          }
        }
      ))
    };
  }
}