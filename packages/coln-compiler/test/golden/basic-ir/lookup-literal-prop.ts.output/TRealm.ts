import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      IntFact: (a: number) => Runtime.MutableProp,
      StringFact: (a: string) => Runtime.MutableProp,
      intFact: Runtime.Ref<null>,
      stringFact: Runtime.Ref<null>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        IntFact: (a: number) => {
          return (new runtime.ViewProp(mstore, "root.IntFact", [a]));
        },
        StringFact: (a: string) => {
          return (new runtime.ViewProp(mstore, "root.StringFact", [a]));
        },
        intFact: (new runtime.ConstRef(null)),
        stringFact: (new runtime.ConstRef(null))
      };
    }
  };
}