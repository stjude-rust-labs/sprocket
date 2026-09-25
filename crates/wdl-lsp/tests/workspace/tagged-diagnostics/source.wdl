#@ except: BashSetSyntax, EmptyOutputs, MetaSections, RequirementsSection

version 1.3

task has_tagged_diagnostics {
    input {
        String unused
    }

    command <<<>>>

    runtime {
        docker: "ubuntu:latest"
    }
}