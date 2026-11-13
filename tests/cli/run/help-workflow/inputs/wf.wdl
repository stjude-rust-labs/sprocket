version 1.2

workflow my_wf {
  input {
    ## Sample identifier
    String sample_id
    Int threads = 4
    File? ref_fasta
    Array[File] reads
  }

  output {
    String result = sample_id
  }
}
