#@ except: BashSetSyntax, EmptyOutputs, MatchingOutputMeta, OutputMetaOrder, MetaDescription, MetaSections
#@ except: ParameterMetaOrder, RequirementsSection, ShellCheck

version 1.3

workflow extraneous_parameter_meta_workflow {
    parameter_meta {
        workflow_input: "A workflow input"
        extra_workflow: "An extra workflow key"
    }

    input {
        String workflow_input
    }
}

struct ExtraneousParameterMetaStruct {
    parameter_meta {
        field: "A field"
        extra_struct: "An extra struct key"
    }

    String field
}

task extraneous_parameter_meta_task {
    parameter_meta {
        task_input: "A task input"
        extra_task: "An extra task key"
    }

    input {
        String task_input
    }

    requirements {
        container: "ubuntu@sha256:cc925e589b7543b910fea57a240468940003fbfc0515245a495dd0ad8fe7cef1"
    }

    command <<<
        echo "~{task_input}"
    >>>
}
