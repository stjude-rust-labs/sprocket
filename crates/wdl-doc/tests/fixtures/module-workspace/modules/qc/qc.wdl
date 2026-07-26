## Quality-control primitives for sequencing reads.
version 1.4

## Classification assigned after evaluating read-level metrics.
enum QcStatus {
    Passed,
    Warning,
    Failed
}

## Summary metrics produced by the quality-control task.
struct QcMetrics {
    ## Total reads observed in the input file.
    Int total_reads

    ## Fraction of reads that passed filtering.
    Float passing_fraction

    ## Overall quality-control classification.
    QcStatus status
}

## Calculates compact quality metrics for a sequencing read file.
##
## The fixture uses fixed values so documentation can be generated
## without requiring executable fixtures.
task collect_qc {
    input {
        ## Sequencing reads to inspect.
        File reads

        ## Minimum read count required to pass.
        Int minimum_reads = 100000
    }

    command <<<
        printf '%s %s\n' '~{reads}' '~{minimum_reads}' > qc-inputs.txt
        printf '125000\n' > total_reads.txt
        printf '0.98\n' > passing_fraction.txt
    >>>

    output {
        ## Number of reads observed.
        Int total_reads = read_int("total_reads.txt")

        ## Fraction of reads passing filters.
        Float passing_fraction = read_float("passing_fraction.txt")
    }

    requirements {
        container: "ubuntu:latest"
        cpu: 1
        memory: "1 GiB"
    }

    meta {
        description: "Computes read-level quality-control metrics."
        outputs: {
            total_reads: "Total reads found in the input.",
            passing_fraction: "Fraction of reads passing filters.",
        }
    }

    parameter_meta {
        reads: "FASTQ file containing sequencing reads."
        minimum_reads: "Minimum acceptable number of reads."
    }
}
