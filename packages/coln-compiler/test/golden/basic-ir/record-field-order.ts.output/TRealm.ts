import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      E: (a: number) => (b: number) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      R: (p: {
        first: number,
        second: number
      }) => (a: Runtime.RowId<"root.E">) => Runtime.MutableSet<Runtime.RowId<"root.R">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        E: (a: number) => {
          return (b: number) => {
            return (new runtime.BaseSet(mbouncer, "root.E", [a, b]));
          };
        },
        R: (p: { first: number, second: number }) => {
          return (a: Runtime.RowId<"root.E">) => {
            return (new runtime.BaseSet(
              mbouncer,
              "root.R",
              [p.first, p.second, a]
            ));
          };
        }
      };
    }
  };
}