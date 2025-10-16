## This tests that accessing members produces appropriate errors/successes.
##
## TaskPreEvaluation (requirements/hints/runtime) has: name, id, attempt, previous
## TaskPostEvaluation (command/output) has: all fields including cpu, memory, etc.

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
    echo "cpu: ~{task.cpu}"
    echo "memory: ~{task.memory}"
    echo "container: ~{task.container}"
  >>>

  output {
    # All task fields available in output.
    String task_name = task.name
    String task_id = task.id
    Int attempt = task.attempt
    Float cpu = task.cpu
    Int memory = task.memory
    String? container = task.container
    Int? previous_memory = task.previous.memory
  }
}
