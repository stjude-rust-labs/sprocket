version 1.2

task strings {
    input {
        String haplotypecallerPassthroughOptions = "embedded \"double\" quote"
    }

    command <<<
        ~{"--haplotypecaller-options " + "\"" + haplotypecallerPassthroughOptions + "\""}
    >>>
}
