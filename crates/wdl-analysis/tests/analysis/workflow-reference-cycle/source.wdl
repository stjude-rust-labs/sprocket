## This is a test of a reference cycle in a workflow.

version 1.3

task my_task {
    input {
        Int x
    }

    command <<<>>>

    output {
        Int y = x
    }
}

workflow test {
    Int a = b

    scatter (x in [a, 1, 2]) {
        call my_task { x }
    }

    Int b = my_task.y[0]
}
