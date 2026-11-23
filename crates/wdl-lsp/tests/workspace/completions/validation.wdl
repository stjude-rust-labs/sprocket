version 1.4

task A {
    input {
        Int a
        
    }

    command <<<>>>

    output {
        Int out = a

        validation {
            out > 0: "~{task.}"
        }
    }
}
