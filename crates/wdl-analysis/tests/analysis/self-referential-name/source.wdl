## This is a test of a self-referential name in a task.

version 1.1

task foo {
    Int a = 1
    Int b = 2
    Int c = a + b + c

    command <<<>>>
}