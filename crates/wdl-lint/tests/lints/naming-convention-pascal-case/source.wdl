#@ except: BashSetSyntax, EmptyOutputs, MetaDescription, MetaSections, MissingParameterMeta, RequirementsSection, RuntimeSection

version 1.3

enum Bad_mixedEnum {
    Bad_mixedChoice,
    GoodChoice,
}

struct Bad_mixedStruct {
    String Bad_mixedMember
    String GoodMember
}

task Bad_mixedTask {
    input {
        String Bad_mixedInput
        String GoodInput
    }

    String Bad_mixedPrivate = "private"
    String GoodPrivate = "private"

    command <<<
        echo "Hello"
    >>>

    output {
        String Bad_mixedOutput = "out"
        String GoodOutput = "out"
    }
}

task GoodTask {
    command <<<>>>
}

workflow Bad_mixedWorkflow {
}
