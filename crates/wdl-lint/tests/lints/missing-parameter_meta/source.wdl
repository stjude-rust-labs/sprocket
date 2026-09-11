#@ except: EmptyOutputs, RuntimeSection, InputName, BashSetSyntax, MetaDescription

version 1.0

workflow test {
    meta {}

    input {}
}

task inputs_with_doc_comments {
    meta {}

    input {
        ## This doc comment suppresses the lint
        String x
    }

    command <<<>>>
}

# This should not have diagnostics for <= 1.2
struct Test {
    String x
}
