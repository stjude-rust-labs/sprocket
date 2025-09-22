version 1.2

workflow multiline_strings2 {
  output {
    # all of these strings evaluate to "hello  world"
    String hw0 = "hello  world"
    String hw1 = <<<hello  world>>>
    String hw2 = <<<   hello  world   >>>
    String hw3 = <<<   
        hello  world>>>
    String hw4 = <<<   
        hello  world
        >>>
    String hw5 = <<<   
        hello  world
    >>>
    # The line continuation causes the newline and all whitespace preceding 'world' to be 
    # removed - to put two spaces between 'hello' and world' we need to put them before 
    # the line continuation.
    String hw6 = <<<
        hello  \
            world
    >>>

    # This string is not equivalent - the first line ends in two backslashes, which is an 
    # escaped backslash, not a line continuation. So this string evaluates to 
    # "hello \\\n  world".
    String not_equivalent = <<<
    hello \\
      world
    >>>
  }
}