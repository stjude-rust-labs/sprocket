version 1.0

struct Foo {
    String after
    String None
}

task foo {
    input {
        # `after` isn't reserved in 1.0
        Int after
    }

    meta {
        None: "xyz"
    }

    command <<<>>>
}
