# This test ensures that a struct definition can be parsed with errors.

version 1.1

struct MyStruct {
    ; # Unknown token
    String a
    ?  # Unexpected token
    Float b


struct Foo {
    Int c
}

struct Person {
    String name
    Int age
}

workflow foo {
    Person person = Person { name }
    Person person = Person { name:}
    Person person = Person { name: "John" }
    Person person = Person { name: "John", }
    Person person = Person { name: "John", age}
    Person person = Person { name: "John", age: 34 }
}
