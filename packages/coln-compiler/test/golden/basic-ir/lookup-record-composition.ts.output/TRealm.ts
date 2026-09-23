import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    key: (a: runtime.RowId<"root.X">) => runtime.MutableRef<{ rank: number }>,
    PayloadAt: (a: number) => runtime.MutableSet<runtime.RowId<"root.PayloadAt">>,
    slot: (x: runtime.RowId<"root.X">) => runtime.MutableRef<runtime.RowId<"root.PayloadAt">>,
    payload: (rank: number) => (a: runtime.RowId<"root.PayloadAt">) => runtime.MutableRef<{
      name: string
    }>,
    E: (a: string) => runtime.MutableSet<runtime.RowId<"root.E">>,
    edge: (x: runtime.RowId<"root.X">) => runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      key: (a: runtime.RowId<"root.X">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.key",
          [a],
          [1, 2],
          {
            flatten: (a: { rank: number }) => {
              return [a.rank];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return { rank: result[0] };
            }
          }
        ));
      },
      PayloadAt: (a: number) => {
        return (new runtime.BaseSet(mstore, "root.PayloadAt", [a]));
      },
      slot: (x: runtime.RowId<"root.X">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.slot",
          [x],
          [1, 2],
          {
            flatten: (a: runtime.RowId<"root.PayloadAt">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.PayloadAt"
              ));
            }
          }
        ));
      },
      payload: (rank: number) => {
        return (a: runtime.RowId<"root.PayloadAt">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.payload",
            [rank, a],
            [2, 3],
            {
              flatten: (a: { name: string }) => {
                return [a.name];
              },
              reconstruct: (result: runtime.WireTuple) => {
                return { name: result[0] };
              }
            }
          ));
        };
      },
      E: (a: string) => {
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