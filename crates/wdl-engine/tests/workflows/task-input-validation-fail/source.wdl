version 1.4

task process {
  input {
    String name
    Int count

    validation {
      length(name) > 0: "Name must not be empty",
      count >= 1: "Count must be at least 1",
    }
  }

  command <<<
  printf "~{name}"
  >>>
}

workflow task_input_validation_fail {
  call process {
    name = "",
    count = 0
  }
}
