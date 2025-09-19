#@ except: UnusedDeclaration
## This is a test of supporting coercions of Map <-> Object/Struct
## See: https://github.com/stjude-rust-labs/wdl/issues/549
version 1.1

struct Foo {
    Int x
    Int y
}

workflow test {
    # Map[X, Y] -> Object where: X -> String
    Map[String, Int] a = { "foo": 1, "bar": 2 }
    Object b = a
    Map[File, Int] c = { "foo": 1, "bar": 2 }
    Object d = c

    # Map[X, Y] -> Struct where: X -> String
    Map[String, Int] e = { "x": 1, "y": 2 }
    Foo f = e
    Map[File, Int] g = { "x": 1, "y": 2 }
    Foo h = g

    # Object -> Map[X, Y] where: String -> X
    Map[String, Int] i = object { foo: 1, bar: 2 }
    Map[File, Int] j = object { foo: 1, bar: 2 }

    # Struct -> Map[X, Y] where: String -> X
    Map[String, Int] k = Foo { x: 1, y: 2 }
    Map[File, Int] l = Foo { x: 3, y: 4 }

    # Invalid coercions (key type is not coercible to or from `String`)
    Object invalid1 = { 1: "foo", 2: "bar" }
    Foo invalid2 = { 1: 1, 2: 2 }
    Map[Int, Int] invalid3 = object { foo: 1, bar: 2 }
    Map[Int, Int] invalid4 = Foo { x: 1, y: 2}
}
