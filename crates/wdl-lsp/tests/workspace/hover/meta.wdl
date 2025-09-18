version 1.2

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
