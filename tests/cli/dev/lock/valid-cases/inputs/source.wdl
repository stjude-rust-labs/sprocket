version 1.3

task docker_good {
    command <<<>>>

    requirements {
        container: "ubuntu:latest"
    }
}

task docker_good2 {
    command <<<>>>

    requirements {
        container: ["debian:bookworm", "debian:trixie"]
    }
}

task oras_good {
    command <<<>>>

    requirements {
        container: "oras://ghcr.io/stjude-rust-labs/sprocket:v0.23.0"
    }
}

task singularity_good {
    command <<<>>>

    requirements {
        container: "library://ubuntu:latest"
    }
}
