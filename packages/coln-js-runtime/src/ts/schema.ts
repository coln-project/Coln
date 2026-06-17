// TODO interface as determined by the compiler

export interface Schema<T> {
  get_json(): string;
}

export interface SchemaProvider<T> {
  schema(): Schema<T>;
}