#@ except: BashSetSyntax, ContainerUri, EmptyOutputs, MetaSections, MutableContainerTag
#@ except: RedundantContainerArray
version 1.4

# TODO: this comment is only flagged when linting is enabled.
task foo {
    command <<<
    >>>

    output {
    }

    requirements {
        container: "ubuntu:latest"
    }
}
