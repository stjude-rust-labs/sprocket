## This is a test of leading whitespace stripping and normalization in command blocks.
version 1.3

task no_whitespace {
    command <<<
        echo "hello"
    >>>
}

task short_leading_whitespace {
    command <<<
        echo "hello"
    >>>
}

task long_leading_whitespace {
    command <<<
        echo "hello"
    >>>
}

task mixed_leading_whitespace {
    command <<<
        echo "hello"
              echo "world"
    >>>
}

task multiple_placeholders_per_line {
    command <<<
        --clip3pAdapterMMp ~{clip_3p_adapter_mmp.left} ~{(if (length(read_twos) != 0)
            then clip_3p_adapter_mmp.right
            else None
        )} \
        --alignEndsProtrude ~{align_ends_protrude.left} "~{(if (length(read_twos) != 0)
            then align_ends_protrude.right
            else None
        )}" \
        --clip3pNbases ~{clip_3p_n_bases.left} ~{(if (length(read_twos) != 0)
            then clip_3p_n_bases.right
            else None
        )} \
        --clip3pAfterAdapterNbases ~{clip_3p_after_adapter_n_bases.left} ~{(if (length(
            read_twos
        ) != 0)
            then clip_3p_after_adapter_n_bases.right
            else None
        )} \
        --clip5pNbases ~{clip_5p_n_bases.left} ~{(if (length(read_twos) != 0)
            then clip_5p_n_bases.right
            else None
        )} \
        --readNameSeparator "~{read_name_separator}"
    >>>
}

task idempotent {
    command <<<
        set -euo pipefail

        n_cores=~{ncpu}
        if ~{use_all_cores}; then
            n_cores=$(nproc)
        fi

        mkdir star_db
        tar -xzf "~{star_db_tar_gz}" -C star_db/ --no-same-owner

        # shellcheck disable=SC2086
        STAR --readFilesIn \
            ~{sep(",", squote(read_one_fastqs_gz))} \
            --alignSJstitchMismatchNmax ~{sep(" ", quote([
                align_sj_stitch_mismatch_n_max.noncanonical_motifs,
                align_sj_stitch_mismatch_n_max.GT_AG_and_CT_AC_motif,
                align_sj_stitch_mismatch_n_max.GC_AG_and_CT_GC_motif,
                align_sj_stitch_mismatch_n_max.AT_AC_and_GT_AT_motif,
            ]))} \
                --clip3pAdapterSeq "~{clip_3p_adapter_seq.left}" ~{(if (length(read_twos)
                != 0)
                then "'" + clip_3p_adapter_seq.right + "'"
                else ""
            )} \
            --clip3pAdapterMMp ~{clip_3p_adapter_mmp.left} ~{(if (length(read_twos) != 0)
                then clip_3p_adapter_mmp.right
                else None
            )} \
                --alignEndsProtrude ~{align_ends_protrude.left} "~{(if (length(read_twos)
                != 0)
                then align_ends_protrude.right
                else None
            )}" \
            --clip3pNbases ~{clip_3p_n_bases.left} ~{(if (length(read_twos) != 0)
                then clip_3p_n_bases.right
                else None
            )} \
                --clip3pAfterAdapterNbases ~{clip_3p_after_adapter_n_bases.left} ~{(if (
                length(read_twos) != 0
            )
                then clip_3p_after_adapter_n_bases.right
                else None
            )} \
            --clip5pNbases ~{clip_5p_n_bases.left} ~{(if (length(read_twos) != 0)
                then clip_5p_n_bases.right
                else None
            )} \
            --readNameSeparator "~{read_name_separator}" \
            --clipAdapterType "~{clip_adapter_type}" \
            foo.bam
    >>>
}
