version 1.3

task already_locked {
    command <<<>>>

    requirements {
        # Shouldn't show up in the lockfile
        container: "ubuntu@sha256:5e275723f82c67e387ba9e3c24baa0abdcb268917f276a0561c97bef9450d0b4"
    }
}

task already_locked_singularity {
    command <<<>>>

    requirements {
        # Also shouldn't show up in the lockfile
        container: "library://ubuntu:sha256.7a63c14842a5c9b9c0567c1530af87afbb82187444ea45fd7473726ca31a598b"
    }
}
