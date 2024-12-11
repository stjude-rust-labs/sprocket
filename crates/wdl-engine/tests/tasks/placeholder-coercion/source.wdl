version 1.2

task placeholder_coercion {
  File x = "/hij"
  Int? i = None

  command <<<>>>

  output {
    Boolean is_true1 = "~{"abc"}" == "abc"
    Boolean is_true2 = "~{x}" == "/hij"
    Boolean is_true3 = "~{5}" == "5"
    Boolean is_true4 = "~{3.141}" == "3.141000"
    Boolean is_true5 = "~{3.141 * 1E-10}" == "0.000000"
    Boolean is_true6 = "~{3.141 * 1E10}" == "31410000000.000000"
    Boolean is_true7 = "~{i}" == ""
  }
}
