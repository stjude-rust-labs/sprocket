## This is a test that partial doc comments on struct members do NOT silence
## the `MetaSections` lint's `parameter_meta` requirement.

version 1.3

## A struct with only some members documented.
struct PartialDocs {
    ## Documented member.
    String foo
    String bar
}