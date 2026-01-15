version 1.3

task default_option {
  input {
    String? s
  }

  command <<<
    printf ~{default="foobar" s} > result1
    printf ~{if defined(s) then "~{select_first([s])}" else "foobar"} > result2
    printf ~{select_first([s, "foobar"])} > result3
  >>>
  
  output {
    Boolean is_true1 = read_string("result1") == read_string("result2")
    Boolean is_true2 = read_string("result1") == read_string("result3")
  }
}
