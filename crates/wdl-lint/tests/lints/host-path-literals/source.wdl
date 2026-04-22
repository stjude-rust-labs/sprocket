#@ except: MetaDescription, MetaSections, ParameterMetaMatched
#@ except: RequirementsSection, MatchingOutputMeta

version 1.2

task run_tool {
    input {
        File data_rel = "data/input.txt"
        File data_abs = "/etc/host/input.txt"
        File? optional_abs = "/etc/host/optional.txt"
        #@ except: HostPathLiterals
        File suppressed_abs = "/etc/host/suppressed.txt"
        Directory folder_rel = "data/folder"
        Directory folder_abs = "/etc/host/folder"
        String text_abs = "/etc/host/not-a-file"
        File win_drive_abs = "C:\\host\\input.txt"
        File win_drive_forward = "C:/host/input.txt"
        File unc_abs = "\\\\server\\share\\input.txt"
    }

    File private_abs = "/etc/host/private.txt"

    command <<<
        echo "run"
    >>>

    output {
        File result = "/etc/host/output.txt"
    }

    requirements {
        container: "ubuntu@sha256:abc"
    }
}

workflow test_workflow {
    input {
        File wf_input_abs = "/etc/host/wf_input.txt"
        Directory wf_input_rel = "wf/input"
    }

    File workflow_private_abs = "/etc/host/wf_private.txt"

    call run_tool
}
