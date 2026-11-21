version 1.4

import "structs.wdl"
import "foo.wdl" as lib

workflow main {
    input {
        Person person
    }

    call lib.greet as t { input: person }

    output {
        String result = t.name
    }
}
