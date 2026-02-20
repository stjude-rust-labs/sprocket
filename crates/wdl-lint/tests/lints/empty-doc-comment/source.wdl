#@ except: MetaSections

version 1.2

##
struct Foo {}

##

##
struct Bar {
    String baz
}

## This is fine - not empty
struct Qux {}

# This is a regular comment, not a doc comment
struct Normal {}

##   
struct WhitespaceOnly {}

## Multiple

##
##

struct MultipleEmpty {}
