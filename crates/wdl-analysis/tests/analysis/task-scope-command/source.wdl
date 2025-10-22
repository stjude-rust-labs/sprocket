## Test that task variable has all members in command section.

version 1.3

task test_command_scope {
  requirements {
    memory: 2000000000
    cpu: 2
  }

  command <<<
    # Post-evaluation fields are available: `name`, `id`, `attempt`, `max_retries`,
    # `previous`, `cpu`, `memory`, `container`, `gpu`, `fpga`, `disks`
    echo "name: ~{task.name}"
    echo "id: ~{task.id}"
    echo "attempt: ~{task.attempt}"
    echo "max_retries: ~{task.max_retries}"
    echo "cpu: ~{task.cpu}"
    echo "memory: ~{task.memory}"
    echo "container: ~{select_first([task.container, 'none'])}"
    echo "gpu: ~{sep(',', task.gpu)}"
    echo "fpga: ~{sep(',', task.fpga)}"
    echo "disks: ~{write_json(task.disks)}"
    echo "previous_memory: ~{select_first([task.previous.memory, 0])}"
  >>>
}
