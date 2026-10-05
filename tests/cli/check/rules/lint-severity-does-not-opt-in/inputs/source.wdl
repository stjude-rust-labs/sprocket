#@ except: BashSetSyntax, ContainerUri, EmptyOutputs, MetaSections, MutableContainerTag
#@ except: RedundantContainerArray
version 1.3

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
