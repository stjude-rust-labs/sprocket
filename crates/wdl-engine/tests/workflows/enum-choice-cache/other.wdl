version 1.3

enum Foo {
    Bar = "qux"
}

task test {
    input {
        Foo foo
    }

    command <<<>>>

    output {
        Foo bar = foo
    }
}
