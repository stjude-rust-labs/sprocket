## Test case to ensure we don't regress https://github.com/stjude-rust-labs/sprocket/issues/636
version 1.3

task serde_array_json {
  input {
    Map[String, Int] string_to_int
  }

  command <<<
    python <<CODE
    import json
    import sys
    with open("~{write_json(string_to_int)}") as j:
      d = json.load(j)
      json.dump(list(d.keys()), sys.stdout)
    CODE
  >>>

  output {
    Array[String] keys = read_json(stdout())
  }
  
  requirements {
    container: "python:latest"
  }
}
