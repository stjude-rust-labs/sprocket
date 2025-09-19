version 1.2

task mem {
  input {
    Array[String] array
  }

  Int array_length = length(array)
  # choose how much memory to use for a task
  String memory = if array_length > 100 then "2GB" else "1GB"

  command <<<
  >>>

  requirements {
    memory: memory
  }
}

workflow ternary {
  input {
    Boolean morning
  }

  call mem { array = ["x", "y", "z"] }

  output {
    # Choose whether to say "good morning" or "good afternoon"
    String greeting = "good ~{if morning then "morning" else "afternoon"}"
  }
}