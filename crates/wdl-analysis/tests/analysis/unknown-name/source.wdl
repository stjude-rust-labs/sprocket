#@ except: UnusedDeclaration
## This is a test of an unknown name in a task.

version 1.1

task foo {
    String a = "hello"
    Int b = 5
    String d = c

    command <<<>>>
}
