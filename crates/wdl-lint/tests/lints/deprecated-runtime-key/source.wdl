#@ except: BashSetSyntax, EmptyOutputs, UnknownRuntimeKeys, MetaDescription, MetaSections
#@ except: MutableContainerTag, MissingParameterMeta, ExtraneousParameterMeta, ParameterMetaOrder, RecommendedRuntimeKeys, ShellCheck

version 1.1

task deprecated_runtime_key {
    runtime {
        docker: "ubuntu:latest"
    }

    command <<<
        echo "hello"
    >>>
}
