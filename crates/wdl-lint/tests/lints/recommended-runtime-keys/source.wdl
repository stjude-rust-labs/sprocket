#@ except: BashSetSyntax, ContainerUri, RedundantContainerArray, EmptyOutputs, UnknownRuntimeKeys, MetaDescription, MutableContainerTag
#@ except: MetaSections, MissingParameterMeta, ExtraneousParameterMeta, ParameterMetaOrder, RuntimeSection, ShellCheck

version 1.0

task missing_two_keys {
    runtime {}

    command <<<
        echo "hello"
    >>>
}

task missing_one_key {
    runtime {
        docker: "ubuntu:latest"
    }

    command <<<
        echo "hello"
    >>>
}
