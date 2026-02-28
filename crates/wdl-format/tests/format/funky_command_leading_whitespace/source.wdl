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
              --clip3pAdapterMMp ~{clip_3p_adapter_mmp.left} ~{(if (length(read_twos) != 0) then clip_3p_adapter_mmp.right else None)} \
              --alignEndsProtrude ~{align_ends_protrude.left} "~{(if (length(read_twos) != 0) then align_ends_protrude.right else None)}" \
              --clip3pNbases ~{clip_3p_n_bases.left} ~{(if (length(read_twos) != 0) then clip_3p_n_bases.right else None)} \
              --clip3pAfterAdapterNbases ~{clip_3p_after_adapter_n_bases.left} ~{(if (length( read_twos
              ) != 0) then clip_3p_after_adapter_n_bases.right else None)} \
              --clip5pNbases ~{clip_5p_n_bases.left} ~{(if (length(read_twos) != 0) then clip_5p_n_bases.right else None)} \
              --readNameSeparator "~{read_name_separator}"
    >>>
}