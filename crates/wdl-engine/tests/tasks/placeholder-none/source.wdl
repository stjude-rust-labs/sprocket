version 1.2

task placeholder_none {
  command <<<>>>

  output {
    String? foo = None
    # The expression in this string results in an error (calling `select_first` on an array 
    # containing no non-`None` values) and so the placeholder evaluates to the empty string and 
    # `s` evalutes to: "Foo is "
    String s = "Foo is ~{select_first([foo])}"
  }
}
