import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    E: (a: number) => (b: number) => runtime.MutableSet<runtime.RowId<"root.E">>,
    R: (p: {
      first: number,
      second: number
    }) => (a: runtime.RowId<"root.E">) => runtime.MutableSet<runtime.RowId<"root.R">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      E: (a: number) => {
        return (b: number) => {
          return (new runtime.BaseSet(mstore, "root.E", [a, b]));
        };
      },
      R: (p: { first: number, second: number }) => {
        return (a: runtime.RowId<"root.E">) => {
          return (new runtime.BaseSet(
            mstore,
            "root.R",
            [p.first, p.second, a]
          ));
        };
      }
    };
  }
}