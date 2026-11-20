## This is a test of type mismatches in a task.

#@ except: UnusedDeclaration

version 1.1

struct Foo {
    Int x
}

task foo {
    Int a = true
    String b = 5
    Array[String] c = { 1: "one", 2: "two" }
    Array[Int] d = [true, false, true]
    Map[Int, String] e = { "a": 1, "b": 2, "c": 3 }
    Array[Int] f = [1, true, false]
    Map[String, Int] g = { "a": 1, "b": true, "c": 3 }
    Foo h = Foo { x: [1] }
    Map[Int, String] i = { 1: "1", true: "2", 3: "3" }

    command <<<>>>
}
