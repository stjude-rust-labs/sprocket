version 1.3

workflow multiline_strings1 {
  output {
    String s = <<<
      This is a
      multi-line string!
    >>>
  }
}