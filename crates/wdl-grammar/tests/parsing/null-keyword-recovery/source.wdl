# https://github.com/stjude-rust-labs/sprocket/issues/1324

version 1.0

workflow repro {
    call foo {
        something: null
    }
}