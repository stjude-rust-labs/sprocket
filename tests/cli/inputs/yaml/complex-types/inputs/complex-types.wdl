## This is a representative WDL workflow to test the behavior of the inputs --yaml command
## This will show the necessary inputs required in yaml format necessary to run this command in the stdout

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
    }

    call foo as my_call
}
