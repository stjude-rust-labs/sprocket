version 1.3

enum Type {
    A,
    B,
    C
}

task hello {
    input {
        String? name
        Type my_enum = Type.A
    }

    command <<<
        echo "Hello, ~{name} of type ~{my_enum}!"
    >>>

}

workflow main {
    call hello {
        input:
            name = "Alice",
            my_enum = Type.B
    }
}
