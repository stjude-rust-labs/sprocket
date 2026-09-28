#@ except: BashSetSyntax, EmptyOutputs, MetaDescription, MetaSections, MissingParameterMeta, RequirementsSection, RuntimeSection

version 1.3

enum Bad_mixedEnum {
    Bad_mixedChoice,
    good_choice,
}

struct Bad_mixedStruct {
    String Bad_mixedMember
    String good_member
}

task Bad_mixedTask {
    input {
        String Bad_mixedInput
        String good_input
    }

    String Bad_mixedPrivate = "private"
    String good_private = "private"

    command <<<
        echo "Hello"
    >>>

    output {
        String Bad_mixedOutput = "out"
        String good_output = "out"
    }
}

task good_task {
    command <<<>>>
}

workflow Bad_mixedWorkflow {
}
