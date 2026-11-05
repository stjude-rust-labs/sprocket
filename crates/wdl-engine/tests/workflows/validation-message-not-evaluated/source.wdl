version 1.4

task echo_value {
  input {
    Array[Int] values

    validation {
      # The message would fail to evaluate if the condition was `false`
      length(values) > 0: "~{values[100]}",
    }
  }

  command <<<
  echo ~{sep(" ", values)}
  >>>

  output {
    String out = read_string(stdout())

    validation {
      # The message would fail to evaluate if the condition was `false`
      out == "1 2 3": "~{values[100]}",
    }
  }
}

workflow test {
  input {
    Array[Int] values = [1, 2, 3]

    validation {
      # The message would fail to evaluate if the condition was `false`
      length(values) == 3: "~{values[100]}",
    }
  }

  call echo_value { values }

  output {
    String out = echo_value.out

    validation {
      # The message would fail to evaluate if the condition was `false`
      out == "1 2 3": "~{values[100]}",
    }
  }
}
