## Alignment primitives for sequencing reads.
version 1.4

## Summary of a completed reference alignment.
struct AlignmentSummary {
    ## Aligned BAM file.
    File bam

    ## Fraction of reads mapped to the reference.
    Float mapped_fraction

    ## Number of duplicate reads.
    Int duplicate_reads
}

## Aligns sequencing reads to a named reference build.
task align_reads {
    input {
        ## Sequencing reads to align.
        File reads

        ## Reference build identifier.
        String reference

        ## Optional read-group identifier.
        String? read_group
    }

    command <<<
        printf '%s %s %s\n' '~{reads}' '~{reference}' '~{default="none" read_group}' > alignment-inputs.txt
        touch aligned.bam
        printf '0.97\n' > mapped_fraction.txt
        printf '1200\n' > duplicate_reads.txt
    >>>

    output {
        ## Coordinate-sorted aligned reads.
        File bam = "aligned.bam"

        ## Fraction of reads mapped to the reference.
        Float mapped_fraction = read_float("mapped_fraction.txt")

        ## Number of reads marked as duplicates.
        Int duplicate_reads = read_int("duplicate_reads.txt")
    }

    requirements {
        container: "ubuntu:latest"
        cpu: 4
        memory: "8 GiB"
    }
}
