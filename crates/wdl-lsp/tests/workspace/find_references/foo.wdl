version 1.2

import "structs.wdl"

task greet {
    input {
        Person person
    }

    command <<<
        echo "~{person.name}"
    >>>

    output {
        String name = read_string(stdout())
    }
}
