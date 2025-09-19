version 1.2

import "lib.wdl" as lib

task greet {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}"
    >>>
}

workflow main {
    input {
        String name = "world"
    }

    #@ except: UnusedCall
    call greet as t1 { input: name = name }
    # abbreviated syntax
    call greet as t2 { name }

    call lib.add as t3 { input:
        a = 1,
        b = 2,
    }

    Person p = Person {
        name: "test",
        age: 1,
    }

    call lib.process { input: person = p }

    #@ except: UnusedDeclaration
    String p_name = p.name

    output {
        Int result = t3.result
    }
}
