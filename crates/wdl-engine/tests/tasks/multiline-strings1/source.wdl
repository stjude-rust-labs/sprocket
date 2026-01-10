version 1.3

task multiline_strings1 {
  command <<<>>>

  output {
    String s = <<<
      This is a
      multi-line string!
    >>>
  }
}