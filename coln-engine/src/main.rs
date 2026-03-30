use std::fs;

use geomerge::ir::FlatTheory;

fn main() {
    let input = fs::read_to_string("data/paths.json").unwrap();
    let val: FlatTheory = serde_json::from_str(&input).unwrap();
    println!("{:?}", val);
}
