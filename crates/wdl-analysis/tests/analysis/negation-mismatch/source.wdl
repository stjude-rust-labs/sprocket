## This is a test of a type mismatch for the negation operator.

version 1.1

task not {
    Int a = 1
    Int b = -a
    String c = "1"
    Int d = -c

    command <<<>>>
}
