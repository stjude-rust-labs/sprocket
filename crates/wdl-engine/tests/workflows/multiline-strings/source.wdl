version 1.2

workflow multiline_strings1 {
  output {
    String s = <<<
      This is a
      multi-line string!
    >>>
  }
}