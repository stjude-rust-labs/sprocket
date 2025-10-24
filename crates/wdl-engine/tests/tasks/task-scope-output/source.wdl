version 1.3

task test_output_scope {
  requirements {
    memory: 256000000
    cpu: 2
  }

  command <<<
    echo "test"
  >>>

  output {
    # Post-evaluation fields are available including previous
    String task_name = task.name
    String task_id = task.id
    Int attempt = task.attempt
    Float cpu = task.cpu
    Int memory = task.memory
    Int? previous_memory = task.previous.memory
    Float? previous_cpu = task.previous.cpu
  }
}
