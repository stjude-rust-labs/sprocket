#@ except: MetaSections, RequirementsSection, ContainerUri, EmptyOutputs, BashSetSyntax

version 1.3

task good {
    input {
        Int cpu = 4
        String memory = "8 GiB"
    }

    command <<<>>>

    requirements {
        cpu: cpu
        memory: memory
    }
}

task good2 {
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

task good3 {
    command <<<>>>

    requirements {
        memory: if task.attempt == 0 then "8 GiB" else "~{8 * (task.attempt + 1)} GiB"
    }
}

task good4 {
    command <<<>>>

    requirements {
        memory: if task.attempt == 0 then "8 GiB" else "16 GiB"
    }
}

task good5 {
    input {
        Int mem_size_gib = 8
    }

    command <<<>>>

    requirements {
        memory: "~{mem_size_gib} GiB"
    }
}

task good6 {
    command <<<>>>

    requirements {
        # Static values are fine in other attributes
        container: "ubuntu:latest"
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