import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    rank: (a: runtime.RowId<"root.X">) => runtime.MutableRef<number>,
    E: (a: number) => runtime.MutableSet<runtime.RowId<"root.E">>,
    edge: (x: runtime.RowId<"root.X">) => runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      rank: (a: runtime.RowId<"root.X">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.rank",
          [a],
          [1, 2],
          {
            flatten: (a: number) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return result[0];
            }
          }
        ));
      },
      E: (a: number) => {
        return (new runtime.BaseSet(mstore, "root.E", [a]));
      },
      edge: (x: runtime.RowId<"root.X">) => {
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