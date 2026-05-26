version 1.3

struct Person {
    parameter_meta {
        name: "Name of the person"
    }

    String name
}

task example {
    parameter_meta {
        message: "Text to be printed"
    }

    input {
        String message
        Person person
    }

    String n = person.name

    command <<<
        echo "~{message}"
    >>>
}

workflow hover_meta {
    Person p = Person {
        name: "foo",
    }
}

task object_meta {
    parameter_meta {
        name: {
            description: "The name of the person to greet",
            something_else: true
        }
        message: {
            description: "Text to be printed",
            help: "Use double quotes for multi-word values."
        }
        only_help: {
            help: "Only the help string is provided here."
        }
    }

    input {
        String name
        String message
        String only_help
    }

    command <<<
        echo "~{name} ~{message} ~{only_help}"
    >>>
}
