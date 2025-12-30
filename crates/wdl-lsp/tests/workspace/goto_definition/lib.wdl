version 1.3

enum Priority {
    Low,
    Medium,
    High
}

task add {
    input {
        Int a
        Int b
    }

    command <<<
        echo $((~{a} + ~{b}))
    >>>

    output {
        Int result = read_int(stdout())
    }
}

struct Person {
    String name
    Int age
}

workflow process {
    input {
        Person person
    }

    output {
        String name = person.name
    }
}
