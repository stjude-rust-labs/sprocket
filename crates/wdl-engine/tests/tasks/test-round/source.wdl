version 1.3

task test_round {
  input {
    Int i1
  }

  Int i2 = i1 + 1
  Float f1 = i1 + 0.49
  Float f2 = i1 + 0.50

  command <<<>>>
  
  output {
    Array[Boolean] all_true = [round(f1) == i1, round(f2) == i2]
  }
}