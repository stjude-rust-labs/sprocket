## Test that task variable has correct members in runtime section.

version 1.3

task test_runtime_scope {
  runtime {
    # Pre-evaluation fields are available.
    memory: if task.name == "test" then "2 GB" else "1 GB"
    cpu: task.attempt + 1
    docker: if task.id != "" then "ubuntu:latest" else "debian:latest"
    disks: if select_first([task.previous.memory, 0]) > 0 then "20 GiB" else "10 GiB"
    gpu: select_first([task.previous.gpu, false])
  }

  command <<<
    echo "test"
  >>>
}
