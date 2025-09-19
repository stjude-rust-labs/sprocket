version 1.2

task true_false_ternary {
  input {
    String message
    Boolean newline
  }

  command <<<
    # these two commands have the same result
    printf "~{message}~{true="\n" false="" newline}" > result1
    printf "~{message}~{if newline then "\n" else ""}" > result2
  >>>

  output {
    Boolean is_true = read_string("result1") == read_string("result2")
  }
}
