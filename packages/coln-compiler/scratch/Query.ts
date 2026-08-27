export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.E", [a]));
      },
      selected: (new runtime.TableCellRef.View(store, "TRealm.selected", []))
    };
    this.pairOfEs = new runtime.Query(
      {
        vars: [
          ["a", { tag: "String" }],
          ["b", { tag: "Int" }],
          ["c", { tag: "RowId", value: "TRealm.E" }],
          ["d", { tag: "RowId", value: "TRealm.E"}],
        ],
        props: [
          {
            tag: "PAtom",
            value: {
              entity: "TRealm.E",
              rowId: { tag: "LocalVar", value: 2 },
              values: [{ tag: "LocalVar", value: 0}, { tag: "LocalVar", value: 1 }]
            }
          },
          {
            tag: "PAtom",
            value: {
              entity: "TRealm.E",
              rowId: { tag: "LocalVar", value: 3 },
              values: [{ tag: "LocalVar", value: 0}, { tag: "LocalVar", value: 1 }]
            }
          }
        ]
      },
      (result) => {
        return {
          payload: { name: result[0], rank: result[1] },
          first: result[2],
          second: result[3]
        };
      }
    )
  }
}
