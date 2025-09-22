#@ except: UnusedDeclaration
## This is a simple test of an argument type mismatch.

version 1.1

task test {
    String x = sub("foo", 1, "bar")
    command <<<>>>
}
