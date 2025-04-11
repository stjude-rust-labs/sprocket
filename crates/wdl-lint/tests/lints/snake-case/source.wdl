#@ except: MetaDescription, MatchingOutputMeta, ExpectedRuntimeKeys

version 1.0

workflow BadWorkflow {
    meta {}

    Float badPrivateDecl = 3.14
    call BadTask
    call good_task

    output {}
}

task BadTask {
    meta {}

    parameter_meta {
        BadInput: "not a good input"
        other_bad_input: "also not a good input"
    }

    input {
        String BadInput
        Int other_bad_input = 13
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

    parameter_meta {
        good_input: "a good input"
        other_good_input: "also a good input"
    }

    input {
        String good_input
        Int other_good_input = 42
    }

    Array[Int] good_private_decl = [1, 2, 3]

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
