# This is a test of struct definitions.

version 1.1

# Test for an empty struct.
struct Empty {}

# Test for a struct with primitive types.
struct PrimitiveTypes {
    # Booleans
    Boolean a
    Boolean? b

    # Ints
    Int c
    Int? d
    
    # Floats
    Float e
    Float? f
    
    # Strings
    String g
    String? h
    
    # Files
    File i
    File? j

}

# Test for a struct with complex types.
struct ComplexTypes {
    # Maps
    Map[Boolean, String] a
    Map[Int?, Array[String]] b
    Map[Float, Map[String, Array[Array[File]]]] c
    Map[String, Pair[Array[String], Map[String, String]]] d
    Map[File, File] e

    # Arrays
    Array[Boolean] f
    Array[Array[Float]] g
    Array[Map[String, Object]] h
    Array[Array[Array[Array[Array[File?]]]]] i
    Array[CustomType] j

    # Pairs
    Pair[Boolean, Boolean] k
    Pair[Pair[Pair[String?, String], Integer], Float] l
    Pair[Map[String?, Pair[String, String]], Int?] m
    Pair[Array[String], Array[String?]] n

    # Object
    Object o

    # Custom types
    MyType p
    MyType? q
}
