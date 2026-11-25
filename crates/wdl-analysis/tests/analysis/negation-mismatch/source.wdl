## This is a test of a type mismatch for the negation operator.

#@ except: UnusedDeclaration

version 1.1

task not {
    Int a = 1
    Int b = -a
    Boolean c = true
    Int d = -c

    command <<<>>>
}
