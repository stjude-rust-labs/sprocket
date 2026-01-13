version 1.3

task ternary {
  input {
    Boolean morning
    Array[String] array = ["x", "y", "z"]
  }

  Int array_length = length(array)
  # choose how much memory to use for a task
  String memory = if array_length > 100 then "2GB" else "1GB"

  command <<<
  >>>

  requirements {
    memory: memory
  }

  output {
    # Choose whether to say "good morning" or "good afternoon"
    String greeting = "good ~{if morning then "morning" else "afternoon"}"
  }
}
