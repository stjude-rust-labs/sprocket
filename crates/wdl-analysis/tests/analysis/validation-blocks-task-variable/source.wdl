## This is a test of the `task` variable in validation blocks.

version 1.4

task t {
    input {
        Int a

        validation {
            task.name != "": "the `task` variable is not allowed in input validation",
            a > 0: "ok",
        }
    }

    command <<<>>>

    output {
        Int out = a

        validation {
            task.return_code == 0: "the `task` variable is allowed in output validation",
            out == a: "ok (~{task.name})",
        }
    }
}
