## This is a test of valid validation blocks.
## No diagnostics are expected; in particular, `checked` is used only by the
## input validation block and should not be reported as unused.

version 1.4

task make_label {
    input {
        String name
        Int age = 0
        Int checked = age + 1
        env String prefix = ""

        validation {
            length(name) > 0: "Task input 'name' must not be empty",
            age >= 0: "Task input 'age' (~{age}) must not be negative",
            checked > 0: "Task input 'checked' must be positive",
        }
    }

    Int doubled = age * 2

    command <<<
        printf "~{prefix}~{name}:~{doubled}"
    >>>

    output {
        String label = read_string(stdout())

        validation {
            length(label) > 1: "Task output 'label' must not be empty",
            task.return_code == 0: "Task must succeed (~{name})",
        }
    }
}

task only_validation {
    command <<<>>>

    output {
        validation {
            true: "never fails",
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
        Boolean flag = age > 10

        validation {
            length(name) > 0 && name != "": "Workflow input 'name' must not be empty",
            age >= 0: if age > 100 then "too old" else "Workflow input 'age' must not be negative",
            flag || !flag: "always true",
        }
    }

    call make_label { name, age }

    scatter (i in range(2)) {
        call make_label as inner { name, age = i }
    }

    if (age > 1) {
        Int nested = age
    }

    output {
        String label = make_label.label
        Array[String] labels = inner.label
        Int? maybe = nested

        validation {
            length(label) > 1: "Workflow output 'label' must not be empty",
            length(labels) == 2: "Expected two labels for ~{name}",
        }
    }
}
