#@ except: SnackCase, ThisIsNotARealRule, MetaSections

version 1.1

workflow test {
  input {
    String message = "Hello, World!"
  }

  output {
    String out = message
  }
}
