## This is a test of a type mismatch for the logical OR operator.

version 1.1

task not {
    Boolean a = true
    Boolean b = a || a
    String c = "true"
    Boolean d = a || c || b

    command <<<>>>
}
