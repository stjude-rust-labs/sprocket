version 1.3

task glob {
  input {
    Int num_files
  }

  command <<<
    for i in {1..~{num_files}}; do
        printf ${i} > file_${i}.txt
    done
  >>>

  output {
    Array[File] outfiles = glob("*.txt")
    Int last_file_contents = read_int(outfiles[num_files-1])
  }
}
