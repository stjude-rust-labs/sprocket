## Utilities to greet a person

version 1.3

## The docs for `greet`
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
