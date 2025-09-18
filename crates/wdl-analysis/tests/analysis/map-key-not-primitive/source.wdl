#@ except: UnusedDeclaration
## This is a test for a non-primitive map key.

version 1.1

struct Foo {
    Int x
}

task test {
    Foo f = Foo { x: 1 }
    Map[Int, String] a = { f: "foo" }

    command <<<>>>
}