## This is a test of validation blocks in input and output sections.

version 1.4

task make_label {
    input {
        String name
        Int age = 0
        env String prefix = ""

        validation {
            length(name) > 0: "Task input 'name' must not be empty",
            age >= 0: "Task input 'age' (~{age}) must not be negative",
        }
    }

    command <<<
        printf "~{prefix}~{name}:~{age}"
    >>>

    output {
        String label = read_string(stdout())

        validation {
            length(label) > 1: "Task output 'label' must not be empty"
        }
    }
}

task only_validation {
    command <<<>>>

    output {
        validation {
            false: "always fails",
        }
    }
}

task empty_validation {
    input {
        validation {}
    }

    command <<<>>>

    output {
        validation {
        }
    }
}

workflow validation_workflow {
    input {
        String name
        Int age

        validation {
            length(name) > 0 && name != "": "Workflow input 'name' must not be empty",
            age >= 0: if age > 100 then "too old" else "Workflow input 'age' must not be negative",
            { "a": 1 }["a"] == 1: "map literal condition",
        }
    }

    call make_label { name, age }

    output {
        String label = make_label.label

        validation {
            length(label) > 1: "Workflow output 'label' must not be empty",
        }
    }
}
