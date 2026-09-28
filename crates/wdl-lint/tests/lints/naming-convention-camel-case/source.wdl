#@ except: BashSetSyntax, EmptyOutputs, MetaDescription, MetaSections, MissingParameterMeta, RequirementsSection, RuntimeSection

version 1.3

enum Bad_mixedEnum {
    Bad_mixedChoice,
    goodChoice,
}

struct Bad_mixedStruct {
    String Bad_mixedMember
    String goodMember
}

task Bad_mixedTask {
    input {
        String Bad_mixedInput
        String goodInput
    }

    String Bad_mixedPrivate = "private"
    String goodPrivate = "private"

    command <<<
        echo "Hello"
    >>>

    output {
        String Bad_mixedOutput = "out"
        String goodOutput = "out"
    }
}

task goodTask {
    command <<<>>>
}

workflow Bad_mixedWorkflow {
}
