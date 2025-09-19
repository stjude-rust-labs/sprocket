version 1.2

import "../test-scatter/source.wdl" as scat

task make_name {
  input {
    String first
    String last
  }

  command <<<
  printf "~{first} ~{last}"
  >>>

  output {
    String name = read_string(stdout())
  }
}

workflow nested_scatter {
  input {
    Array[String] first_names = ["Bilbo", "Gandalf", "Merry"]
    Array[String] last_names = ["Baggins", "the Grey", "Brandybuck"]
    Array[String] salutations = ["Hello", "Goodbye"]
  }

  Array[String] honorifics = ["Mr.", "Wizard"]

  # the zip() function creates an array of pairs
  Array[Pair[String, String]] name_pairs = zip(first_names, last_names)
  # the range() function creates an array of increasing integers
  Array[Int] counter = range(length(name_pairs))

  scatter (name_and_index in zip(name_pairs, counter) ) {
    Pair[String, String] names = name_and_index.left

    # Use a different honorific for even and odd items in the array
    # `honorifics` is accessible here
    String honorific = honorifics[name_and_index.right % 2]
    
    call make_name {
      first = names.left,
      last = names.right
    }

    scatter (salutation in salutations) {
      # `names`, and `salutation` are all accessible here
      String short_greeting = "~{salutation} ~{honorific} ~{names.left}"
      call scat.say_hello { greeting = short_greeting }

      # the output of `make_name` is also accessible
      String long_greeting = "~{salutation} ~{honorific} ~{make_name.name}"
      call scat.say_hello as say_hello_long { greeting = long_greeting }

      # within the scatter body, when we access the output of the
      # say_hello call, we get a String
      Array[String] messages = [say_hello.msg, say_hello_long.msg]
    }

    # this would be an error - `salutation` is not accessible here
    # String scatter_saluation = salutation
  }

  # Outside of the scatter body, we can access all of the names that
  # are inside the scatter body, but the types are now all Arrays.
  # Each of these outputs will be an array of length 3 (the same
  # length as `name_and_index`).
  output {
    # Here we are one level of nesting away from `honorific`, so
    # the implicitly created array is one level deep
    Array[String] used_honorifics = honorific

    # Here we are two levels of nesting away from `messages`, so
    # the array is two levels deep
    Array[Array[Array[String]]] out_messages = messages

    # This would be an error - 'names' is not accessible here
    # String scatter_names = names  
  }
}