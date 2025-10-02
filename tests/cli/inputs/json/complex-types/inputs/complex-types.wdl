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
        Foo foo = Foo {
            foo: 42,
            bar: "bar",
            baz: bar,
        }
        Bar bar = Bar {
            foo: "file.txt",
            bar: "dir",
            baz: Baz {
                foo: true,
                bar: 4.2,
            }
        }
        Baz baz = Baz {
            foo: false,
            bar: 1.2,
        }
        Int? x
        Array[Float] y = [1.2, 3.4, -0.1]
        String empty = ""
        String interpolated = "weirdly nested string with interpolation: ~{empty}"
    }

    call foo as my_call
}
