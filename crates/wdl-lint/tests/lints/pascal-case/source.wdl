## This is a test of ensuring struct names are PascalCase.

version 1.1

struct this_is_a_bad_name {
    Int x
}

struct thisIsAlsoABadName {
    Int x
}

struct This_Is_Bad_Too {
    Int x
}

struct ThisNameIsAGoodOne {
    Int x
}

#@ except: PascalCase
struct excepted_name {
    Int x
}
