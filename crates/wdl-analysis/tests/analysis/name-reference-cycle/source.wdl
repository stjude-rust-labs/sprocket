## This is a test of a name reference cycle in a task.

#@ except: UnusedDeclaration

version 1.1

task foo {
    Int a = b
    Int b = c
    Int c = a

    command <<<>>>
}
