#@ except: MetaSections, RequirementsSection, EmptyOutputs, BashSetSyntax

version 1.3

struct fooBar {
    Int badName
}

task Say_hello {
    input {
        String greeting_message
    }

    command <<<
        echo "~{greeting_message}"
    >>>
}
