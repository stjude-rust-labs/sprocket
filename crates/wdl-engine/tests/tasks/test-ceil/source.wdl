version 1.2

task test_ceil {
  input {
    Int i1
  }

  Int i2 = i1 + 1
  Float f1 = i1
  Float f2 = i1 + 0.1

  command <<<>>>
  
  output {
    Array[Boolean] all_true = [ceil(f1) == i1, ceil(f2) == i2]
  }
}
