import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    E: (a: {
      name: string,
      rank: number
    }) => runtime.MutableSet<runtime.RowId<"root.E">>,
    selected: runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      E: (a: { name: string, rank: number }) => {
        return (new runtime.BaseSet(mstore, "root.E", [a.name, a.rank]));
      },
      selected: (new runtime.BaseTableRef(
        mstore,
        "root.selected",
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