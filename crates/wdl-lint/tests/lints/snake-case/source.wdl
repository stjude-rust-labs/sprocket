#@ except: BlankLinesBetweenElements, DescriptionMissing, LineWidth, NonmatchingOutput, SectionOrdering, RuntimeSectionKeys
## Test SnakeCase rule

version 1.0

workflow BadWorkflow {
    meta {}
    output {}
    Float badPrivateDecl = 3.14
    call BadTask
    call good_task
}

task BadTask {
    meta {}

    input {
        String BadInput
        Int other_bad_input = 13
    }

    parameter_meta {
        BadInput: "not a good input"
        other_bad_input: "also not a good input"
    }

    command <<<
        echo "Hello World"
    >>>
    output {
        File badOut = "out.txt"
    }
    runtime {}
}

task good_task {
    meta {}
    Array[Int] good_private_decl = [1, 2, 3]
    input {
        String good_input
        Int other_good_input = 42
    }

    parameter_meta {
        good_input: "a good input"
        other_good_input: "also a good input"
    }

    command <<<
        echo "Hello World"
    >>>
    output {
        File good_out = "out.txt"
    }
    runtime {}
}

struct GoodStruct {
    String good_field
    String bAdFiElD  # unfortunately, `convert-case` doesn't understand sarcasm case
    #@ except: SnakeCase
    String OK
}
