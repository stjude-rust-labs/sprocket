#@ except: RequirementsSection, MetaDescription, RuntimeSection, OutputSection
#@ except: ElementSpacing

version 1.2

workflow foo {
    meta {}
    input {}
    output {}
    parameter_meta {}
    scatter (x in range(3)) {
        call bar
    }
    call baz
}

task bar {
    meta {}
    command <<< >>>
    parameter_meta {}
}

task baz {
    meta {}
    command <<< >>>
    output {}

}

task qux {
    requirements {}
    meta {}
    command <<<>>>
    output {}
}

struct Quux {
    meta {}
    parameter_meta {
        x: "an integer"
    }
    Int x
}

struct Corge {
    parameter_meta {
        x: "an integer"
    }
    meta {}
    Int x
}

struct Grault {
    Int x
    meta {}
    parameter_meta {
        x: "an integer"
        y: "an integer"
    }
    Int y
}
