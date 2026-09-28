#@ except: BashSetSyntax, EmptyOutputs, MetaDescription, MetaSections, MissingParameterMeta, RequirementsSection, RuntimeSection

version 1.3

enum Bad_mixedEnum {
    Bad_mixedChoice,
    GOOD_CHOICE,
}

struct Bad_mixedStruct {
    String Bad_mixedMember
    String GOOD_MEMBER
}

task Bad_mixedTask {
    input {
        String Bad_mixedInput
        String GOOD_INPUT
    }

    String Bad_mixedPrivate = "private"
    String GOOD_PRIVATE = "private"

    command <<<
        echo "Hello"
    >>>

    output {
        String Bad_mixedOutput = "out"
        String GOOD_OUTPUT = "out"
    }
}

task GOOD_TASK {
    command <<<>>>
}

workflow Bad_mixedWorkflow {
}
