version 1.3

enum Type {
    A,
    B,
    C,
}

task hello {
    input {
        String? name
        Type my_enum = Type.A
        Type? my_enum2 = Type.B
    }

    command <<<
        echo "Hello, ~{name} of type ~{my_enum} and type ~{my_enum2}!"
    >>>
}

workflow main {
    call hello { input:
        name = "Alice",
        my_enum = Type.B,
        my_enum2 = Type.C,
    }
}
