#@ except: BlankLinesBetweenElements, DescriptionMissing, LineWidth, SectionOrdering, RuntimeSectionKeys, TrailingComma

version 1.1

struct Mystruct {
    String a
    Int b
}

workflow foo {
    meta {}

    call bar {
        input: a="something"
    }

    call bar as ba

    call bar as ba2 {input:
        a="something", b="somethingelse"
    }

    call bar as ba3 { input: a = "something"}
    output {}
}

task bar {
    meta {}
    parameter_meta {
        a: ""
        b: ""
    }
    input {
        String a
        String? b
    }
    command <<<
    >>>
    runtime {}
    output {}
}
