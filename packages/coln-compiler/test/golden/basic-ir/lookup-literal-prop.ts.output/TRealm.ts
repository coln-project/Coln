import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    IntFact: (a: number) => runtime.MutableProp,
    StringFact: (a: string) => runtime.MutableProp,
    intFact: runtime.MutableRef<null>,
    stringFact: runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      IntFact: (a: number) => {
        return (new runtime.BaseProp(mstore, "root.IntFact", [a]));
      },
      StringFact: (a: string) => {
        return (new runtime.BaseProp(mstore, "root.StringFact", [a]));
      },
      intFact: (new runtime.ConstRef(null)),
      stringFact: (new runtime.ConstRef(null))
    };
  }
}