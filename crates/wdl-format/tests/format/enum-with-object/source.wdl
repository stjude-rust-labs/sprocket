version 1.3

enum Foobar {
    Qoox,
    Foo,
    Qiil,
    Baz,
    Quux,
}

workflow test {
    Foobar s = Foobar.Qoox
    Foobar v = Foobar.Qiil
}
