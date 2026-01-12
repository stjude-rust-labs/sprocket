version 1.3

struct A {
  String s
}

struct B {
  A a_struct
  Int i
}

struct C {
  String s
}

struct D {
  C a_struct
  Int i
}

workflow struct_to_struct {
  B my_b = B {
    a_struct: A { s: 'hello' },
    i: 10
  }
  # We can coerce `my_b` from type `B` to type `D` because `B` and `D`
  # have members with the same names and compatible types. Type `A` can
  # be coerced to type `C` because they also have members with the same
  # names and compatible types.
  
  output {
    D my_d = my_b
  }
}