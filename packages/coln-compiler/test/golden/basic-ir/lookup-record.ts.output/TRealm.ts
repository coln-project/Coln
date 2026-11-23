import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      E: (a: {
        name: string,
        rank: number
      }) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      selected: Runtime.Ref<Runtime.RowId<"root.E">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        E: (a: { name: string, rank: number }) => {
          return (new runtime.InductiveViewSet(
            mstore,
            "root.E",
            [a.name, a.rank]
          ));
        },
        selected: (new runtime.BaseTableRef(
          mstore,
          "root.selected",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.E">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.E"
              ));
            }
          }
        ))
      };
    }
  };
}