version 1.3

task test {
  meta {
    description: "Task that shows how to use the implicit 'task' variable"
  }

  command <<<
    echo "Task name: ~{task.name}"
    echo "Task id: ~{task.id}"
    echo "Task description: ~{task.meta.description}"
    echo "Task container: ~{select_first([task.container, "ubuntu:latest"])}"
    exit 1
  >>>
  
  output {
    String name = task.name
    String id = task.id
    String description = task.meta.description
    String? container = select_first([task.container, "ubuntu:latest"])
    Int? return_code = task.return_code
  }

  requirements {
    container: "ubuntu:latest"
    return_codes: [0, 1]
  }
}
