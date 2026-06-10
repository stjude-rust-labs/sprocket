## This is a test of too few arguments to a function.

#@ except: UnusedDeclaration

version 1.1

task test {
    String x = sub("foo")

    command <<<>>>
}
