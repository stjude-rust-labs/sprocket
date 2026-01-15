version 1.3

import "lib.wdl" as lib

workflow main {
    input {
        String name = "world"
    }

    Person p = Person {
        name: "test",
        age: read_int(p.name),
    }

    call greet {
    }
    call lib.greet as t {
    }

    output {
        String result = read_string(t.out)
    }
}

task greet {
    input {
        String name
    }

    command <<<
        echo "hello ~{name}"
    >>>
}
