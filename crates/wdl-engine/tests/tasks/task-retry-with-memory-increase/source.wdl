version 1.3

task test_retry_memory {
  requirements {
    # Start with 128MB, double on each retry: 128MB * 2^attempt
    memory: 128000000 * (2 ** task.attempt)
    max_retries: 2
  }

  command <<<
    # Fail on first two attempts, succeed on third
    if [ ~{task.attempt} -lt 2 ]; then
      echo "Attempt ~{task.attempt}: Memory=~{task.memory}, FAILING"
      exit 1
    else
      echo "Attempt ~{task.attempt}: Memory=~{task.memory}, SUCCESS"
      echo "Previous memory was: ~{select_first([task.previous.memory, 0])}"
      exit 0
    fi
  >>>

  output {
    Int final_attempt = task.attempt
    Int final_memory = task.memory
    Int? previous_memory = task.previous.memory
  }
}
