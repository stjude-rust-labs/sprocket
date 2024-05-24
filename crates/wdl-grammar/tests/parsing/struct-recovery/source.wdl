# This test ensures that a struct definition can be parsed with errors.

version 1.1

struct MyStruct {
    ; # Unknown token
    String a
    ?  # Unexpected token
    Float b
    struct   # Unexpected keyword
    Int c
}
