#@ except: BashSetSyntax, EmptyOutputs, MetaDescription, MetaSections, MissingParameterMeta, ExtraneousParameterMeta, ParameterMetaOrder
#@ except: RequirementsSection, ShellCheck

version 1.3

task mutable_container_tag {
    requirements {
        container: "ubuntu:latest"
    }

    command <<<
        echo "hello"
    >>>
}
