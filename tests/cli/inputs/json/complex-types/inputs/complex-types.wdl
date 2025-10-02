## This is a representative WDL workflow to test the behavior of the default inputs command
## This will show the necessary inputs required in json format necessary to run this command in the stdout

version 1.2

struct Foo {
    Int int
    String str
    Bar bar
}

struct Bar {
    File file
    Directory dir
    Baz baz
}

struct Baz {
    Boolean bool
    Float float
}

task foo_task {
    input {
        Foo foo
        Map[String, String] string_map = {
            "key1": "value1",
            "key2": "value2",
        }
        Map[Int, String] int_map = {
            1: "one",
            2: "two",
            3: "three",
        }
    }

    command <<<>>>
}

workflow test {
    meta {
        allowNestedInputs: true
    }

    input {
        Foo foo = Foo {
            int: 42,
            str: "bar",
            bar: bar,
        }
        Bar bar = Bar {
            file: "file.txt",
            dir: "dir",
            baz: Baz {
                bool: true,
                float: 4.2,
            }
        }
        Baz baz = Baz {
            bool: false,
            float: 1.2,
        }
        Int? x
        File required_file
        Directory required_directory
        String required_string
        String default_string = "I have a default value"
        Array[Float] y = [1.2, 3.4, -0.1]
        String empty = ""
        String interpolated = "weirdly nested string with interpolation: ~{empty}"
    }

    call foo_task as my_call
}
