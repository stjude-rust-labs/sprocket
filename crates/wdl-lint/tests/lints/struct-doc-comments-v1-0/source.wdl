## This is a test of doc comments on structs and their fields missing comments in WDL 1.0.

version 1.0

struct NoDocs {
    String value
}

## A fully documented struct.
struct FullDocs {
    ## A documented value.
    String value
}

## A struct with undocumented fields.
struct StructOnly {
    String value
}

struct FieldsOnly {
    ## A documented value.
    String value
}

## A partially documented struct.
struct PartialDocs {
    ## A documented first value.
    String first
    String second
}