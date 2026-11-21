## Utilities to greet a person

version 1.4

task greet {
    input {
        String name
    }

    command <<<
        echo "hello ~{name}"
    >>>

    output {
        String out = read_string(stdout())
    }
}

struct Person {
    String name
    Int age
}
