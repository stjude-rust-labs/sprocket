#@ except: UnusedDeclaration
## This is a test of a type mismatch for the logical AND operator.

version 1.1

task not {
    Boolean a = true
    Boolean b = a && a
    Int c = 1
    Boolean d = a && c && b

    command <<<>>>
}
