version 1.3

task concat_optional {
  input {
    String salutation = "hello"
    String? name1
    String? name2 = "Fred"
  }

  command <<<>>>

  output {
    # since name1 is undefined, the evaluation of the expression in the placeholder fails, and the
    # value of greeting1 = "nice to meet you!"
    String greeting1 = "~{salutation + ' ' + name1 + ' '}nice to meet you!"

    # since name2 is defined, the evaluation of the expression in the placeholder succeeds, and the
    # value of greeting2 = "hello Fred, nice to meet you!"
    String greeting2 = "~{salutation + ' ' + name2 + ', '}nice to meet you!"
  }
}
