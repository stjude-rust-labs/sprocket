version 1.2

task foobar {
  input {
    File infile
  }

  command <<<
  wc -l '~{infile}' | awk '{print $1}'
  >>>

  output {
    Int results = read_int(stdout())
  }

  requirements {
    container: "ubuntu:latest"
  }
}

workflow other {
  input {
    Boolean b = false
    File? f
  }

  if (b && defined(f)) {
    call foobar { infile = select_first([f]) }
  }

  output {
    Int? results = foobar.results
  }
}