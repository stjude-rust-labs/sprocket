#@ except: BashSetSyntax, DeprecatedRuntimeSection, RequirementsSection, EmptyOutputs, MetaSections

## This is a test for a missing description in a `meta` section.

version 1.3

task foo {
    meta {}

    command <<<>>>
}

workflow bar {
    meta {}

    output {}
}

struct Baz {
    meta {}

    String x
}


## This doc comment counts as a description
task foo2 {
    meta {}

    command <<<>>>
}

## Same here
struct Baz2 {
    meta {}

    String x
}