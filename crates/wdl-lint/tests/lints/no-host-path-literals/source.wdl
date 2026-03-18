#@ except: MetaDescription, MetaSections, ParameterMetaMatched, RequirementsSection
#@ except: RuntimeSection, MatchingOutputMeta, TodoComment, ShellCheck

version 1.2

task align {
    input {
        File reference_ok = "refs/hg38.fa"
        File reference_abs = "/data/references/hg38.fa"
        File? optional_abs = "/data/references/optional.fa"
        #@ except: NoHostPathLiterals
        File suppressed_abs = "/suppressed/ref.fa"
        Directory index_ok = "refs/index"
        Directory index_abs = "/data/references/index"
        String text_abs = "/tmp/not-a-file"
    }

    File private_abs = "/private/ref.fa"

    command <<<
        cat /container/internal/path.txt
        echo "aligning"
    >>>

    output {
        File result_bam = "/results/output.bam"
    }

    requirements {
        container: "ubuntu@sha256:abc"
    }
}

workflow test_workflow {
    input {
        File wf_input_abs = "/workflow/input/ref.fa"
        Directory wf_input_rel = "workflow/input"
    }

    File workflow_private_abs = "/workflow/private/ref.fa"

    call align
}
