#@ except: UnusedDeclaration
## This is a test of an invalid access.

version 1.1

task test {
    String a = "foo"
    String x = a.bar
    command <<<>>>
}
