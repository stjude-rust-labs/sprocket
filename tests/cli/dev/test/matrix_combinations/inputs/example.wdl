version 1.1

import "flag_filter.wdl"

task so_many_options {
    input {
        File bam
        File bam_index
        FlagFilter bitwise_filter
        String prefix = basename(bam, ".bam")
        Boolean paired_end = true
        Boolean collated = false
        Boolean retain_collated_bam = false
        Boolean fast_mode = !retain_collated_bam
        Boolean append_read_number = true
        Boolean output_singletons = false
        Boolean fail_on_unexpected_reads = false
    }
    
    command <<<>>>
}

task has_a_reference {
    input {
        File bam
        File bam_index
        File ref_fasta
        File ref_fasta_index
        FlagFilter bitwise_filter
        String prefix = basename(bam, ".bam")
        Boolean output_singletons
        Boolean paired_end
    }
    
    command <<<>>>
}
