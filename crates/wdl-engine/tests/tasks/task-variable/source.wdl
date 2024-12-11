version 1.2

task test {
  meta {
    description: "Task that shows how to use the implicit 'task' variable"
  }

  command <<<
    echo "Task name: ~{task.name}"
    echo "Task id: ~{task.id}"
    echo "Task description: ~{task.meta.description}"
    echo "Task container: ~{task.container}"
    exit 1
  >>>
  
  output {
    String name = task.name
    String id = task.id
    String description = task.meta.description
    String? container = task.container
    Int? return_code = task.return_code
  }

  requirements {
    return_codes: [0, 1]
  }
}
