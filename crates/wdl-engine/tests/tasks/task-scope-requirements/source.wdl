version 1.3

task test_requirements_scope {
  requirements {
    # Pre-evaluation fields are available: name, id, attempt, previous
    memory: if task.name == "test_requirements_scope" then 256000000 else 128000000
    cpu: if task.attempt > 1 then task.attempt + 1 else 2
    container: if task.id != "" then "ubuntu:latest" else "debian:latest"
  }

  command <<<
    echo "task_name=~{task.name}"
    echo "task_attempt=~{task.attempt}"
    echo "task_cpu=~{task.cpu}"
    echo "task_memory=~{task.memory}"
    echo "previous_memory=~{select_first([task.previous.memory, 0])}"
  >>>

  output {
    String task_name = read_string(stdout())
    Int attempt = task.attempt
    Float cpu = task.cpu
    Int memory = task.memory
    Int? previous_memory = task.previous.memory
  }
}
