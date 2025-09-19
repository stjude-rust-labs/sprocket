#@ except: UnusedDeclaration
## This is a test for a name that doesn't refer to a struct member.

version 1.1

struct Foo {
    Int x
}

task test {
    Foo a = Foo { x: 1, y: "2" }
    String b = a.y

    command <<<>>>
}
