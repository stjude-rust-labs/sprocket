## Test that task variable has all members in output section.

version 1.3

task test_output_scope {
  requirements {
    memory: 2000000000
    cpu: 2
  }

  command <<<
    echo "test"
  >>>

  output {
    # Post-evaluation fields are available.
    String task_name = task.name
    String task_id = task.id
    Int attempt = task.attempt
    Int max_retries = task.max_retries
    Float cpu = task.cpu
    Int memory = task.memory
    String? container = task.container
    Array[String] gpu = task.gpu
    Array[String] fpga = task.fpga
    Map[String, Int] disks = task.disks
    Int? previous_memory = task.previous.memory
    Float? previous_cpu = task.previous.cpu
    String? previous_container = task.previous.container
    Array[String]? previous_gpu = task.previous.gpu
    Array[String]? previous_fpga = task.previous.fpga
    Map[String, Int]? previous_disks = task.previous.disks
    Int? previous_max_retries = task.previous.max_retries
  }
}
