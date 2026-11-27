import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      IntFact: (a: number) => Runtime.MutableProp,
      StringFact: (a: string) => Runtime.MutableProp,
      intFact: Runtime.MutableRef<null>,
      stringFact: Runtime.MutableRef<null>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        IntFact: (a: number) => {
          return (new runtime.BaseProp(mbouncer, "root.IntFact", [a]));
        },
        StringFact: (a: string) => {
          return (new runtime.BaseProp(mbouncer, "root.StringFact", [a]));
        },
        intFact: (new runtime.ConstRef(null)),
        stringFact: (new runtime.ConstRef(null))
      };
    }
  };
}