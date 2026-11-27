use std::env;

fn main() {
    // The desktop runner constructs emulator state on the native main thread.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo::rustc-link-arg-bin=systemless=/STACK:8388608");
    }
}
