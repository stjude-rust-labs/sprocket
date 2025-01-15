# Comments are allowed before version
version 1.2

# This is how you
# write a long
# multiline
# comment

task task_with_comments {
  input {
    Int number  # This comment comes after a variable declaration
  }

  # This comment will not be included within the command
  command <<<
    # This comment WILL be included within the command after it has been parsed
    echo ~{number * 2}
  >>>

  output {
    Int result = read_int(stdout())
  }
    
  requirements {
    container: "ubuntu:latest"
  }
}

workflow workflow_with_comments {
  input {
    Int number
  }

  # You can have comments anywhere in the workflow
  call task_with_comments { number }
  
  output { # You can also put comments after braces
    Int result = task_with_comments.result
  }
}