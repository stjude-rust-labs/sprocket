version 1.2

task test_floor {
  input {
    Int i1
  }

  Int i2 = i1 - 1
  Float f1 = i1
  Float f2 = i1 - 0.1

  command <<<>>>
  
  output {
    Array[Boolean] all_true = [floor(f1) == i1, floor(f2) == i2]
  }
}
