#@ except: BashSetSyntax, EmptyOutputs, UnknownRuntimeKeys, DeprecatedRuntimeKey, RecommendedRuntimeKeys, MatchingOutputMeta, OutputMetaOrder
#@ except: MetaDescription, MetaSections, CallInputKeyword

version 1.3

workflow BadWorkflow {
    meta {}

    Float badPrivateDecl = 3.14
    call BadTask { input:
        BadInput = "something",
    }
    call good_task { input:
        good_input = "something",
    }

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

task BadButAllowedTask {
    meta {}

    parameter_meta {}

    command <<<
    >>>

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
    #@ except: NamingConvention
    String OK
}

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

#@ except: NamingConvention
struct excepted_name {
    Int x
}

enum log_level {
    Good,
    bad_choice,
    Also_Bad,
}

enum GoodEnum {
    Red,
    Green,
}

struct v1 {
    Int x
}

struct BadButAllowedStruct {
    Int Bad_but_allowed
}
