version 1.3

enum Status {
  Active = "active",
  Inactive = "inactive"
}

task enum_type_name_command_interpolation {
  command <<<
    # This should fail: cannot interpolate type name references
    echo ~{Status}
  >>>

  requirements {
    container: "ubuntu:latest"
  }

  output {
    String result = read_string(stdout())
  }
}
