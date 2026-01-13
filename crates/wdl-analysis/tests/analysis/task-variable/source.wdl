## This is a test for the `task` variable in a task block or command section.
## Example taken directly from the WDL spec.

version 1.3

task test_runtime_info_task {
  meta {
    description: "Task that shows how to use the implicit 'task' declaration"
  }

  command <<<
    echo "Task name: ~{task.name}"
    echo "Task description: ~{task.meta.description}"
    echo "Task container: ~{task.container}"
    echo "Available cpus: ~{task.cpu}"
    echo "Available memory: ~{task.memory / (1024 * 1024 * 1024)} GiB"
    echo "Not a member: ~{task.not_a_member}"
    exit 1
  >>>
  
  output {
    Boolean at_least_two_gb = task.memory >= (2 * 1024 * 1024 * 1024)
    Int? return_code = task.return_code
  }
  
  requirements {
    container: ["ubuntu:latest", "quay.io/ubuntu:focal"]
    memory: "2 GiB"
    return_codes: [0, 1]
  }
}