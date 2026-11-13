version 1.3

import "lib.wdl" as lib

## A task with documented inputs.
task greet {
    input {
        ## The name of the person to greet
        String name
        ## An optional punctuation mark
        String punctuation = "!"
        Boolean? loud
    }

    command <<<
        echo "~{name}~{punctuation}"
    >>>

    output {
        String message = stdout()
    }
}

## A workflow with documented inputs.
workflow accumulate {
    input {
        ## The values to accumulate
        Array[Int] values
        Int initial = 0
    }

    output {
        Int total = initial
    }
}

workflow caller {
    call greet {
        name = "world",
        punctuation = "?"
    }

    call lib.lib_task {
        file = "data.txt",
        flag = true
    }

    call accumulate {
        values = [1, 2]
    }

    call missing_task {
        value = 1
    }
}
