## This is a test for https://github.com/stjude-rust-labs/wdl/issues/315
## This test will output an array rather than emit a diagnostic

version 1.2

struct Sample {
  String kind
  String name
}

workflow run {
  Array[Sample] samples = [
    Sample {
      kind: "normal",
      name: "sample_one",
    },
    Sample {
      kind: "tumor",
      name: "sample_two",
    },
    Sample {
      kind: "normal",
     name: "sample_three",
    },
    Sample {
      kind: "tumor",
      name: "sample_four",
    },
  ]

  scatter (sample in samples) {
    if (sample.kind == "normal") {
      String name = sample.name
    }
  }

  output {
    Array[String] names = select_all(name)
  }
}