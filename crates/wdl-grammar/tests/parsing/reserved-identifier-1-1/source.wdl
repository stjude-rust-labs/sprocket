version 1.1

struct Foo {
    String after
    String None
}

task foo {
    input {
        Int after
    }

    meta {
        None: "xyz"
    }

    command <<<>>>
}
