version 1.4

task make_label {
  input {
    String name
    Int age

    validation {
      length(name) > 0: "Task input 'name' must not be empty",
      age >= 0: "Task input 'age' must not be negative",
    }
  }

  command <<<
  printf "~{name}:~{age}"
  >>>

  output {
    String label = read_string(stdout())

    validation {
      length(label) > 1: "Task output 'label' must not be empty",
    }
  }
}

workflow validation_example {
  input {
    String name
    Int age

    validation {
      length(name) > 0: "Workflow input 'name' must not be empty",
      age >= 0: "Workflow input 'age' must not be negative",
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
