## This tests that accessing members produces appropriate errors/successes.
##
## The `task` variable is available in both pre-evaluation contexts
## (requirements/hints/runtime) and post-evaluation contexts (command/output),
## but with different fields available in each context.

version 1.3

task test_invalid_member {
  requirements {
    # Available in pre-evaluation.
    memory: if task.name == "test" then 1000000000 else 2000000000

    # Available in pre-evaluation.
    container: if task.id == "test-1" then "ubuntu:latest" else "debian:latest"

    # Available in pre-evaluation.
    cpu: task.attempt + 1

    # Available in pre-evaluation.
    disks: select_first([task.previous.disks, ["10 GiB"]])

    # Not a valid member of `task.previous` (error).
    gpu: task.previous.not_a_member

    # `task.cpu` not available in pre-evaluation (error).
    fpga: task.cpu > 2

    # `task.memory` not available in pre-evaluation (error).
    maxRetries: task.memory / 1000000000
  }

  command <<<
    # All task fields available in command.
    echo "name: ~{task.name}"
    echo "id: ~{task.id}"
    echo "attempt: ~{task.attempt}"
    echo "max_retries: ~{task.max_retries}"
    echo "cpu: ~{task.cpu}"
    echo "memory: ~{task.memory}"
    echo "container: ~{task.container}"
    echo "previous_cpu: ~{select_first([task.previous.cpu, 0])}"
    echo "previous_memory: ~{select_first([task.previous.memory, 0])}"
  >>>

  output {
    # All task fields available in output.
    String task_name = task.name
    String task_id = task.id
    Int attempt = task.attempt
    Int max_retries = task.max_retries
    Float cpu = task.cpu
    Int memory = task.memory
    String? container = task.container
    Int? previous_memory = task.previous.memory
  }
}
