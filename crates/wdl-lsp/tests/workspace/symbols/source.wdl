version 1.3

import "lib.wdl"
import "lib.wdl" as lib_alias

struct Person {
    String name
    Int age
}

task greet {
    input {
        Person person
    }

    String message = "hello ~{person.name}"

    command <<<
        echo "~{message}"
    >>>

    output {
        String out = read_string(stdout())
    }
}

workflow main {
    input {
        Person p
        Boolean condition
        Array[Int] numbers = [
            1,
            2,
            3,
        ]
    }

    if (condition) {
        call greet as greet_in_if { input: person = p }
    }

    scatter (i in numbers) {
        call greet as greet_in_scatter { input: person = p }
    }

    call greet { input: person = p }

    output {
        String result = greet.out
    }
}
