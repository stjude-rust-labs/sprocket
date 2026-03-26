#@ except: MetaSections

version 1.3

##
struct Foo {
    String value
}

##

##
struct Bar {
    String baz
}

# Test: Empty followed by non-empty - should NOT flag (per Serial-ATA's review)
##
## Not empty, this is fine
struct EmptyThenNonEmpty {
    String value
}

## This is fine - not empty
struct Qux {
    String value
}

# This is a regular comment, not a doc comment
struct Normal {
    String value
}

##   
struct WhitespaceOnly {
    String value
}

## Multiple with text - should NOT flag

##
##

struct MultipleWithText {
    String value
}

# Test: Three consecutive empty doc comments (from original issue #632)
##
##
##
struct ThreeEmpty {
    String value
}

## This is a paragraph separator test
##
## The empty line in the middle is valid - used as paragraph separator
struct ParagraphSeparator {
    String value
}

## Good doc block
## with multiple lines
## all having content
struct GoodDocBlock {
    String value
}
