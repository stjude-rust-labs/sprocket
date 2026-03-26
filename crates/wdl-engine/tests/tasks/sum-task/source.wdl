## Example from https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#arrayx
version 1.3

task sum {
  input {
    Array[String]+ ints
  }
  
  command <<<
  printf '~{sep(" ", ints)}' | awk '{tot=0; for(i=1;i<=NF;i++) tot+=$i; print tot}'
  >>>
  
  output {
    Int total = read_int(stdout())
  }
}
