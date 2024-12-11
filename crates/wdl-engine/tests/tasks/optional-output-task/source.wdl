version 1.2

task optional_output {
  input {
    Boolean make_example2
  }

  command <<<
    printf "1" > example1.txt
    if ~{make_example2}; then
      printf "2" > example2.txt
    fi
  >>>
  
  output {
    File example1 = "example1.txt"
    File? example2 = "example2.txt"
    Array[File?] file_array = ["example1.txt", "example2.txt"]
    Int file_array_len = length(select_all(file_array))
  }
}