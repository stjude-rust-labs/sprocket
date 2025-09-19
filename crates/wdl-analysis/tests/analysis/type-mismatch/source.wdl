#@ except: UnusedDeclaration
## This is a test of type mismatches in a task.

version 1.1

struct Foo {
    Int x
}

task foo {
    Int a = "hello"
    String b = 5
    Array[String] c = { 1: "one", 2: "two" }
    Array[Int] d = ["a", "b", "c"]
    Map[Int, String] e = { "a": 1, "b": 2, "c": 3 }
    Array[Int] f = [1, "2", "3"]
    Map[String, String] g = { "a": "1", "b": 2, "c": "3" }
    Foo h = Foo { x: [1] }
    Map[String, String] i = { "a": "1", 0: "2", "c": "3" }

    command <<<>>>
}
