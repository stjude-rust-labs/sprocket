# https://github.com/stjude-rust-labs/sprocket/issues/596
version 1.0

struct Foo {
    Int bar
}

workflow repro {
    Foo foo = Foo {
        bar: 0
    }
}