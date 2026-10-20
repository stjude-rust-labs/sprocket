#@ except: BashSetSyntax, DescriptionLength, EmptyOutputs, MetaDescription, MetaSections
#@ except: MissingParameterMeta, ExtraneousParameterMeta, ParameterMetaOrder, RequirementsSection, ShellCheck

version 1.3

task output_meta_order {
    meta {
        outputs: {
            second: "The second output",
            first: "The first output",
        }
    }

    requirements {
        container: "ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1"
    }

    command <<<
        echo "hello"
    >>>

    output {
        String first = "one"
        String second = "two"
    }
}

workflow output_meta_order_workflow {
    meta {
        outputs: {
            workflow_second: "The second workflow output",
            workflow_first: "The first workflow output",
        }
    }

    input {
        String value = "value"
    }

    output {
        String workflow_first = value
        String workflow_second = value
    }
}

task output_meta_order_with_doc_comments_ok {
    meta {
        outputs: {
            first: "The first output",
            second: "The second output",
        }
    }

    requirements {
        container: "ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1"
    }

    command <<<
        echo "hello"
    >>>

    output {
        ## ignored output with doc comments
        String ignored = "zero"
        String first = "one"
        String second = "two"
    }
}

task output_meta_order_with_doc_comments_err {
    meta {
        outputs: {
            second: "The second output",
            first: "The first output",
        }
    }

    requirements {
        container: "ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1"
    }

    command <<<
        echo "hello"
    >>>

    output {
        ## ignored output with doc comments
        String ignored = "zero"
        String first = "one"
        String second = "two"
    }
}

