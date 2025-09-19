version 1.2

task multiline_strings1 {
  command <<<>>>

  output {
    String s = <<<
      This is a
      multi-line string!
    >>>
  }
}