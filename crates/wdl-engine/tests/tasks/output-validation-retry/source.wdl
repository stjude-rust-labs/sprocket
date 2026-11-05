version 1.4

task test {
  input {
    Int expected = 2
  }

  requirements {
    max_retries: 2
  }

  command <<<
  echo "attempt ~{task.attempt}"
  >>>

  output {
    Int attempt = task.attempt
    Float? previous_cpu = task.previous.cpu
    String out = read_string(stdout())

    validation {
      attempt == expected: "attempt ~{attempt} is not the expected attempt ~{expected}",
      attempt == 0 || defined(previous_cpu): "the previous attempt should be available",
    }
  }
}
