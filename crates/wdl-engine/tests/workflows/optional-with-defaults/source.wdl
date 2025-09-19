version 1.2

task say_hello {
  input {
    String name
    String? salutation = "hello"
  }

  command <<< >>>

  output {
    String greeting = if defined(salutation) then "~{salutation} ~{name}" else name
  }
}

workflow optional_with_default {
  input {
    String name
    Boolean use_salutation
  }
  
  if (use_salutation) {
    call say_hello as hello1 { 
      name = name 
    }
  }

  if (!use_salutation) {
    call say_hello as hello2 {
      name = name,
      salutation = None 
    }
  }

  output {
    String greeting = select_first([hello1.greeting, hello2.greeting])
  }
}