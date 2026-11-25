#@ except: UnusedDeclaration
## This is a test of a type mismatch for the logical NOT operator.

version 1.1

task not {
    Boolean a = true
    Boolean b = !a
    Int c = 1
    Boolean d = !c

    command <<<>>>
}
