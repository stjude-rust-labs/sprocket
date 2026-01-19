version 1.3

task compare_optionals {
  Int i = 1
  Int? j = 1
  Int? k = None

  command <<<>>>

  output {
    # equal values of the same type are equal even if one is optional
    Boolean is_true1 = i == j
    # k is undefined (None), and so is only equal to None
    Boolean is_true2 = k == None
    # these comparisons are valid and evaluate to false
    Boolean is_false1 = i == k
    Boolean is_false2 = j == k
  }
}