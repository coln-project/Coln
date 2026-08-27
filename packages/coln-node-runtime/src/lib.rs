use napi_derive::napi;

#[napi]
pub fn hello() {
    println!("Hello, world!");
}
