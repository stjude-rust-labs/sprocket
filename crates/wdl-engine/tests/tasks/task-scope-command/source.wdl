version 1.3

task test_command_scope {
  requirements {
    container: "ubuntu:latest"
    memory: 256000000
    cpu: 2
  }

  command <<<
    # Post-evaluation fields are available including previous
    echo "name=~{task.name}"
    echo "id=~{task.id}"
    echo "attempt=~{task.attempt}"
    echo "cpu=~{task.cpu}"
    echo "memory=~{task.memory}"
    echo "container=~{select_first([task.container, 'none'])}"
    echo "previous_memory=~{select_first([task.previous.memory, 0])}"
  >>>

  output {
    Array[String] lines = read_lines(stdout())
  }
}
