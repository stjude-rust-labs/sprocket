version 1.2

task input_type_quantifiers {
  input {
    Array[String]  a
    Array[String]+ b
    Array[String]? c
    # If the next line were uncommented it would cause an error
    # + only applies to Array, not File
    #File+ d
    # An optional array that, if defined, must contain at least one element
    Array[String]+? e
  }

  command <<<
    cat '~{write_lines(a)}' >> result
    cat '~{write_lines(b)}' >> result
    ~{if defined(c) then 
    "cat '~{write_lines(select_first([c]))}' >> result"
    else ""}
    ~{if defined(e) then 
    "cat '~{write_lines(select_first([e]))}' >> result"
    else ""}
  >>>

  output {
    Array[String] lines = read_lines("result")
  }
  
  requirements {
    container: "ubuntu:latest"
  }
}
