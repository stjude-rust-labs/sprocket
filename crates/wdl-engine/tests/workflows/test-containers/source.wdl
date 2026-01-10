version 1.3

task single_image_task {
  command <<< printf "hello" >>>

  output {
    String greeting = read_string(stdout())
  }

  requirements {
    container: "ubuntu:latest"
  }
}

task multi_image_task {
  command <<< printf "hello" >>>

  output {
    String greeting = read_string(stdout())
  }

  requirements {
    container: ["ubuntu:latest", "https://gcr.io/standard-images/ubuntu:latest"]
  }
}

workflow test_containers {
  call single_image_task
  call multi_image_task
  output {
    String single_greeting = single_image_task.greeting
    String multi_greeting = multi_image_task.greeting
  }
}