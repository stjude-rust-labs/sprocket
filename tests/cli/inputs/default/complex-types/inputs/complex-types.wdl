## This is a representative WDL workflow to test the behavior of the default inputs command
## This will show the necessary inputs required in json format necessary to run this command in the stdout

version 1.2

struct Foo {
    Int foo
    String bar
    Bar baz
}

struct Bar {
    File foo
    Directory bar
    Baz baz
}

struct Baz {
    Boolean foo
    Float bar
}

task foo {
    input {
        Foo foo
    }

    command <<<>>>
}

workflow test {
    meta {
        allowNestedInputs: true
    }

    input {
        Foo foo
        Bar bar
        Baz baz
        Int? x
        Array[Float] y
        String empty = ""
        String interpolated = "weirdly nested string with interpolation: ~{empty}"
    }

    call foo as my_call
}
