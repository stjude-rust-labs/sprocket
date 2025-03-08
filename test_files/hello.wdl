version 1.0

workflow hello {
  input {
    String name
    Int age
    Boolean is_student
  }

  call say_hello { input: name = name, age = age, is_student = is_student }

  output {
    String message = say_hello.message
  }
}

task say_hello {
  input {
    String name
    Int age
    Boolean is_student
  }

  command {
    echo "Hello, ~{name}! You are ~{age} years old."
    echo "Student status: ~{is_student}"
  }

  output {
    String message = read_string(stdout())
  }
} 