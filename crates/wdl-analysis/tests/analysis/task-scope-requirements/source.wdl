## Test that task variable has correct members in requirements section.

version 1.3

task test_requirements_scope {
  requirements {
    # Pre-evaluation fields are available.
    memory: if task.name == "test" then 1000000000 else 2000000000
    cpu: task.attempt + 1
    container: if task.id != "" then "ubuntu:latest" else "debian:latest"
    disks: if select_first([task.previous.memory, 0]) > 0 then ["20 GiB"] else ["10 GiB"]
    gpu: select_first([task.previous.memory, 0]) > 1000000000
  }

  command <<<
    echo "test"
  >>>
}
