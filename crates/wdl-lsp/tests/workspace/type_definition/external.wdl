version 1.3

struct ExternalPerson {
    String name
}

enum ExternalColor {
    Green,
    Yellow
}

task greet_external {
    input {
        ExternalPerson person
    }

    command <<<
        echo "Hello, ~{person.name}!"
    >>>

    output {
        ExternalPerson out = person
    }
}
