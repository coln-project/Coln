#![deny(clippy::all)]

use napi_derive::napi;

pub mod tuple;

#[napi]
pub fn hello() {
    println!("hello")
}
