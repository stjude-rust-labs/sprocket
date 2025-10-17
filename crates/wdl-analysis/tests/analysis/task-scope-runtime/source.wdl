## Test that task variable has correct members in runtime section.

version 1.3

task test_runtime_scope {
  runtime {
    # Pre-evaluation fields are available in runtime section.
    # task.name - the name of the task
    memory: if task.name == "test_runtime_scope" then "2 GB" else "1 GB"

    # task.attempt - the current attempt number (1-indexed)
    cpu: task.attempt + 1

    # task.id - the unique identifier for this task execution
    docker: if task.id != "" then "ubuntu:latest" else "debian:latest"

    # task.previous.* - access to previous attempt's resource allocations
    # Useful for implementing retry logic with increased resources
    disks: if select_first([task.previous.memory, 0]) > 0 then "20 GiB" else "10 GiB"
    gpu: select_first([task.previous.gpu, false])

    # task.previous.cpu - can be used to scale up CPU on retries
    maxRetries: if select_first([task.previous.cpu, 0]) > 4 then 10 else 5
  }

  command <<<
    echo "test"
  >>>
}
