version 1.4

import "structs.wdl" alias Person as Human
import "foo.wdl" as tools

workflow main {
    input {
        Human human
    }

    call tools.greet as worker { input: person = human }

    output {
        String result = worker.name
    }
}
