#@ except: MetaSections, RequirementsSection, ContainerUri, EmptyOutputs, ExpectedRuntimeKeys, BashSetSyntax

version 1.1

task good {
    input {
        Int cpu = 4
        String memory = "8 GiB"
    }

    command <<<>>>

    runtime {
        cpu: cpu
        memory: memory
    }
}

task bad {
    command <<<>>>

    runtime {
        cpu: 0.5
        memory: "2 GiB"
        disks: 1
    }
}

task bad2 {
    command <<<>>>

    runtime {
        cpu: (0.5)
        memory: if true then "2 GiB" else "1 GiB"
        disks: 1 + 1
    }
}

task bad3 {
    command <<<>>>

    runtime {
        memory: "~{4 + 4} GiB"
    }
}

task bad4 {
    command <<<>>>

    runtime {
        disks: ["2", "/mnt/outputs 4 GiB", "/mnt/tmp 1 GiB"]
    }
}