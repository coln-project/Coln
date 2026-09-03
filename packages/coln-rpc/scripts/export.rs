use specta::Types;
use specta_typescript::Typescript;
use coln_rpc::api::*;

fn main() {
    let mut types = Types::default();

    types.register_mut::<Query>();
    types.register_mut::<QueryResponse>();

    Typescript::default()
        .export_to("./types.ts", &types, specta_serde::Format)
        .unwrap();
}
