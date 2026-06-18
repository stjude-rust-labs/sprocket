## This is a test of an index target that is not an array.

#@ except: UnusedDeclaration

version 1.1

task test {
    String a = "foo"
    String x = a[0]
    command <<<>>>
}
