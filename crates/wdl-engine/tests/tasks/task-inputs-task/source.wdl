version 1.3

task task_inputs {
  input {
    Int i                 # a required input parameter
    String s = "hello"    # an input parameter with a default value
    File? f               # an optional input parameter
    Directory? d = "/etc" # an optional input parameter with a default value
  }

  command <<<
  for i in 1..~{i}; do
    printf "~{s}\n"
  done
  if ~{defined(f)}; then
    cat ~{f}
  fi
  >>>
}