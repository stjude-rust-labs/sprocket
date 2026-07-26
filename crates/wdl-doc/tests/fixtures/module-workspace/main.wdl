## End-to-end genomics analysis workflows.
version 1.4

import "modules/qc/qc.wdl" as qc
import { AlignmentSummary, align_reads } from "modules/alignment/alignment.wdl"

## Supported reference genome builds.
enum ReferenceBuild {
    GRCh37,
    GRCh38
}

## A sequencing sample and its analysis metadata.
struct Sample {
    ## Stable sample identifier.
    String id

    ## Sequencing reads.
    File reads

    ## Reference build used for alignment.
    ReferenceBuild reference

    ## Optional read-group identifier.
    String? read_group
}

## Runs quality control and alignment for one sequencing sample.
##
## This workflow demonstrates namespace and selective imports, structured
## inputs, task calls, and typed outputs.
workflow analyze_sample {
    input {
        ## Sample to analyze.
        Sample sample

        ## Minimum read count accepted by quality control.
        Int minimum_reads = 100000
    }

    call qc.collect_qc {
        input:
            reads = sample.reads,
            minimum_reads = minimum_reads
    }

    call align_reads {
        input:
            reads = sample.reads,
            reference = if sample.reference == ReferenceBuild.GRCh38 then "GRCh38" else "GRCh37",
            read_group = sample.read_group
    }

    output {
        ## Aligned BAM file.
        File bam = align_reads.bam

        ## Alignment summary assembled from task outputs.
        AlignmentSummary alignment = AlignmentSummary {
            bam: align_reads.bam,
            mapped_fraction: align_reads.mapped_fraction,
            duplicate_reads: align_reads.duplicate_reads,
        }

        ## Total reads observed during quality control.
        Int total_reads = collect_qc.total_reads
    }

    meta {
        description: "Runs the standard quality-control and alignment pipeline."
        outputs: {
            bam: "Coordinate-sorted aligned reads.",
            alignment: "Typed summary of alignment outputs.",
            total_reads: "Total input reads observed by quality control.",
        }
    }

    parameter_meta {
        sample: "Sequencing sample and reference metadata."
        minimum_reads: "Minimum read count required by quality control."
    }
}
