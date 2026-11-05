version 1.4

task test {
  requirements {
    max_retries: 1
  }

  command <<<
  echo "attempt ~{task.attempt}"
  >>>

  output {
    String out = read_string(stdout())

    validation {
      task.attempt > 5: "attempt ~{task.attempt} printed `~{out}`",
    }
  }
}
