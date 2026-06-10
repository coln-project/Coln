export type DatabaseId = string;

export type RowId = {
  from: DatabaseId;
  index: number;
}

export namespace RowId {
  export function equal(v0: RowId, v1: RowId): boolean {
    return v0.from == v1.from && v0.index == v1.index;
  }
  
  export function hash(v: RowId): number {
    // todo: make better
    return v.index;
  }
}

export type Value =
  { tag: 'row_id', value: RowId } |
  { tag: 'tuple', value: Tuple }

export namespace Value {
  export function equal(v0: Value, v1: Value): boolean {
    if (v0.tag == 'row_id' && v1.tag == 'row_id') {
      return RowId.equal(v0.value, v1.value);
    } else if (v0.tag == 'tuple' && v1.tag == 'tuple') {
      return Tuple.equal(v0.value, v1.value);
    }
    return true;
  }
  
  export function hash(v: Value): number {
    if (v.tag == 'row_id') {
      return RowId.hash(v.value)
    } else if (v.tag == 'tuple') {
      return Tuple.hash(v.value)
    }
    return 0;
  }
}

export type Tuple = Value[];

export namespace Tuple {
  export function equal(t0: Tuple, t1: Tuple): boolean {
    if (t0.length == t1.length) {
      for (var i = 0; i < t0.length; i++) {
        if (!Value.equal(t0[i], t1[i])) {
          return false;
        }
      }
      return true;
    } else {
      return false
    }
  }

  export function hash(t: Tuple): number {
      var h = 1;
      for (var i = 0; i < t.length; i++) {
        h ^= Value.hash(t[i]);
      }
      return h;
  }
}

export namespace RowId {
  export function validate(x: Value) {
    if (x.tag != 'row_id') {
      throw '${x} is not a row id'
    }
  }
}

export interface ReadonlySet {
  has(x : Value): boolean;
  values(): Iterator<Value>;
}

export interface ReadWriteSet extends ReadonlySet {
  add(): Value;
}

export type RelationId = {
  from: DatabaseId;
  path: string[];
}

export class RelationIndex {
  sets: [Value[], Set<number>][];
  lookup: Map<number, number[]>;
  
  constructor() {
    this.sets = [];
    this.lookup = new Map();
  }

  get(params: Tuple): Set<number> | undefined {
    var indices = this.lookup.get(Tuple.hash(params));
    if (indices) {
      for (const i of indices) {
        const [k,v] = this.sets[i]
        if (Tuple.equal(params, k)) {
          return v;
        }
      }
    }
  }
  
  add(params: Tuple, i: number) {
    const h = Tuple.hash(params);
    const indices = this.lookup.get(h);
    if (indices) {
      for (const i of indices) {
        const [k,v] = this.sets[i]
        if (Tuple.equal(params, k)) {
          v.add(i);
          return;
        }
      }
      let n = this.sets.length;
      let s = new Set([i]);
      this.sets.push([params, s])
      indices.push(n);
    } else {
      let n = this.sets.length;
      let s = new Set([i]);
      this.sets.push([params, s])
      this.lookup.set(h, [n])
    }
  }
}

export class RelTable {
  id: RelationId;
  id_generator: IdGenerator;
  by_id: Map<number, Value[]>;
  by_value: RelationIndex;
  validate: (params: Value[]) => void;
  
  constructor(path: string[], id_generator: IdGenerator, validate: (params: Value[]) => void) {
    this.id = { from: id_generator.name, path };
    this.id_generator = id_generator;
    this.by_id = new Map();
    this.by_value = new RelationIndex();
    this.validate = validate;
  }
  
  apply_to(params: Value[]): AppliedRelTable {
    this.validate(params);
    return new AppliedRelTable(this, params)
  }
}

export class AppliedRelTable implements ReadWriteSet {
  relation: RelTable;
  params: Value[];
  
  constructor(relation: RelTable, params: Value[]) {
    this.relation = relation;
    this.params = params;
  }
  
  has(x: Value): boolean {
    if (x.tag == 'row_id') {
      if (x.value.from != this.relation.id_generator.name) {
        throw 'row id in wrong database'
      }
      const s = this.relation.by_value.get(this.params);
      if (s) {
        return s.has(x.value.index);
      } else {
        return false;
      }
    } else {
      return false;
    }
  }
  
  values(): Iterator<Value> {
    const s = this.relation.by_value.get(this.params);
    if (s) {
      const values: Value[] = [...s.values()].map((i) => ({
        tag: 'row_id',
        value: { from: this.relation.id_generator.name, index: i }
      }));
      return values.values()
    } else {
      return [].values()
    }
  }
  
  add(): Value {
    const i = this.relation.id_generator.fresh();
    this.relation.by_id.set(i.index, this.params);
    const s = this.relation.by_value.get(this.params);
    if (s) {
      s.add(i.index);
    } else {
      this.relation.by_value.add(this.params, i.index);
    }
    return { tag: 'row_id', value: i };
  }
}

export class IdGenerator {
  name: string;
  next: number;
  
  constructor() {
    this.name = "foo";
    this.next = 0;
  }

  fresh(): RowId {
    const i = this.next;
    this.next = i + 1;
    return { from: this.name, index: i };
  }
}
