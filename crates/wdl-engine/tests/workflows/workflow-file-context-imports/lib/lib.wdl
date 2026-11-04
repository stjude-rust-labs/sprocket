version 1.3

struct Wrapper {
  File file
  Array[File] files
}

task read_file {
  input {
    File file
    Wrapper wrapper
  }

  command <<<
    cat '~{file}'
    cat '~{wrapper.file}'
    cat '~{sep(" ", wrapper.files)}'
  >>>

  output {
    Array[String] lines = read_lines(stdout())
  }
}

workflow read_workflow {
  input {
    File file
    Wrapper wrapper
  }

  # Relative to this document, not the caller's
  File local = "data/hello.txt"

  output {
    String contents = read_string(file)
    String wrapper_contents = read_string(wrapper.files[0])
    String local_contents = read_string(local)
  }
}
