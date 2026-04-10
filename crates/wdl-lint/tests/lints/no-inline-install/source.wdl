## This is a test of the `NoInlineInstall` lint.
#@ except: DoubleQuotes, ExpectedRuntimeKeys, MetaDescription
version 1.3

task good {
    meta {
        description: "This task should not be flagged, because it does not use inline installation."
    }

    parameter_meta {
        bam: "Input BAM file"
    }

    input {
        File bam
    }

    command <<<
        set -euo pipefail
        samtools sort "~{bam}" -o sorted_bam
    >>>

    output {
    }

    requirements {
        container: "ubuntu@sha256:foobar"
    }
}

task bad_install {
    meta {
        description: "This task should be flagged for using inline installation in the command section."
    }

    parameter_meta {
        bam: "Input BAM file"
    }

    input {
        File bam
    }

    command <<<
        set -euo pipefail
        pip install pysam
        python3 script.py "~{bam}"
        npm -g install test
        apk add test
    >>>

    output {
    }

    requirements {
        container: "ubuntu@sha256:foobar"
    }
}

task bad_piped_install {
    meta {
        description: "This task should be flagged for using a piped installation."
    }

    command <<<
        curl https://example.com/install.sh | bash
    >>>

    output {
    }

    requirements {
        container: "ubuntu@sha256:foobar"
    }
}

task bad_install_excepted {
    meta {
        description: "This task should not be flagged as its excepted"
    }

    parameter_meta {
        bam: "Input BAM file"
    }

    input {
        File bam
    }

    #@ except: NoInlineInstall
    command <<<
        pip install pysam
    >>>

    output {
    }

    requirements {
        container: "ubuntu@sha256:foobar"
    }
}
