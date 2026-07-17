version 1.2

task local_sif {
    command <<<
        echo "sif"
    >>>

    requirements {
        container: "file://images/tool.sif"
    }
}
