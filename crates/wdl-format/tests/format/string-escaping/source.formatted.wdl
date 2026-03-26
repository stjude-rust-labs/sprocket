version 1.3

task strings {
    input {
        String haplotypecallerPassthroughOptions = "embedded \"double\" quote"
    }

    command <<<
        ~{"--haplotypecaller-options " + "\"" + haplotypecallerPassthroughOptions + "\""}
    >>>
}
