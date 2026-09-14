version 1.3

task strings {
    input {
        String haplotypecallerPassthroughOptions = 'embedded "double" quote'
    }

    command <<<
        ~{"--haplotypecaller-options " + '"' + haplotypecallerPassthroughOptions + '"'}
    >>>
}

task strings_inverted {
    input {
        String haplotypecallerPassthroughOptions = "embedded 'single' quote"
    }

    command <<<
        ~{'--haplotypecaller-options ' + "'" + haplotypecallerPassthroughOptions + "'"}
    >>>
}
