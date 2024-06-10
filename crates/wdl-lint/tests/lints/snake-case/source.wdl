## Test SnakeCase rule

version 1.0

workflow BadWorkflow {
    Float badPrivateDecl = 3.14
    call BadTask
    call good_task
}

task BadTask {
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
    Array[Int] good_private_decl = [1, 2, 3]
    input {
        String good_input
        Int other_good_input = 42
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
}
