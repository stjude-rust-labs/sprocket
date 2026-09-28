#@ except: BashSetSyntax, EmptyOutputs, MetaDescription, MetaSections, MissingParameterMeta, ExtraneousParameterMeta, ParameterMetaOrder
#@ except: RequirementsSection, ShellCheck

version 1.3

task redundant_container_array {
    requirements {
        container: ["ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1"]
    }

    command <<<
        echo "hello"
    >>>
}
