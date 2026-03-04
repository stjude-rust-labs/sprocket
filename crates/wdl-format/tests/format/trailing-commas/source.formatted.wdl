version 1.3

struct Person {
    String name
    Int age
}

enum Status {
    Active,
    Suspended,
}

workflow test {
    meta {
        foo: [
            "bar",
            "baz",
        ]
        bar: {
            baz: "qux",
            qux: "quux",
            quux: "corge",
        }
    }

    Array[String] names = [
        "James",
        "Jimmy",
        "John",
    ]
    Map[String, Int] ages = {
        "James": 34,
        "Jimmy": 55,
        "John": 26,
    }
    Person james = Person {
        name: "James",
        age: 34,
    }
    Array[Person] people = [
        james,
        Person {
            name: "Jimmy",
            age: 55,
        },
        Person {
            name: "John",
            age: 26,
        },
    ]
    Object person = object {
        name: "Jimmy",
        age: 55,
    }

    call test2 {
        foo = "foo",
        bar = "bar",
    }

    hints {
        foo: [
            1,
            2,
            3,
        ]
    }
}

task test2 {
    input {
        String foo
        String bar
    }

    command <<<
    >>>

    output {
        String baz = "baz"
        String qux = "qux"
    }

    hints {
        inputs: input {
            foo: hints {
                min_length: 3,
                max_length: 15,
            },
            bar: hints {
                min_length: 2,
            },
        }
        outputs: output {
            baz: hints {
                min_length: 3,
                max_length: 15,
            },
            qux: hints {
                min_length: 2,
            },
        }
    }
}
