version 1.3

enum type {
    A,
    B,
    C
}

task hello {
    input {
        String? name
        type my_enum = type.A
    }

    command <<<
        echo "Hello, ~{name} of type ~{my_enum}!"
    >>>

}

workflow main {
    call hello {
        input:
            name = "Alice",
            my_enum = type.B
    }
}
