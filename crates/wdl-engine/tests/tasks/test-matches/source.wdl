version 1.2

task contains_string {
  input {
    File fastq
  }
  
  command <<<>>>

  output {
    Boolean is_compressed = matches(basename(fastq), "\\.(gz|zip|zstd)")
    Boolean is_read1 = matches(basename(fastq), "_R1")
  }
}
