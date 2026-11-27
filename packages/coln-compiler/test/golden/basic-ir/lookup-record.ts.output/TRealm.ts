import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      E: (a: {
        name: string,
        rank: number
      }) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      selected: Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        E: (a: { name: string, rank: number }) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a.name, a.rank]));
        },
        selected: (new runtime.BaseTableRef(
          mbouncer,
          "root.selected",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.E">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.E"
              ));
            }
          }
        ))
      };
    }
  };
}