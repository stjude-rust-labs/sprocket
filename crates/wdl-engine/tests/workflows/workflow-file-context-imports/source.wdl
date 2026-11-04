version 1.3

import "lib/lib.wdl" as lib

workflow workflow_file_context_imports {
  File declared = "data/hello.txt"
  String path = "data/hello.txt"

  call lib.read_file {
    file = "data/hello.txt",
    wrapper = Wrapper {
      file: declared,
      files: [path],
    },
  }

  call lib.read_workflow {
    file = path,
    wrapper = Wrapper {
      file: path,
      files: ["data/hello.txt"],
    },
  }

  output {
    Array[String] task_lines = read_file.lines
    String workflow_contents = read_workflow.contents
    String workflow_wrapper_contents = read_workflow.wrapper_contents
    String workflow_local_contents = read_workflow.local_contents
  }
}
