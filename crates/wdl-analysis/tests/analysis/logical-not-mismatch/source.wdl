#@ except: UnusedDeclaration
## This is a test of a type mismatch for the logical NOT operator.

version 1.1

task not {
    Boolean a = true
    Boolean b = !a
    String c = "true"
    Boolean d = !c

    command <<<>>>
}
