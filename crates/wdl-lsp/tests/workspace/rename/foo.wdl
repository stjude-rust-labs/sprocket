version 1.4

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
