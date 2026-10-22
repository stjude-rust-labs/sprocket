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

# This struct should have diagnostics for missing doc comments
struct Test {
    String x
}
