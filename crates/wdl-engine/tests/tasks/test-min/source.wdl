version 1.3

task test_min {
  input {
    Int value1
    Float value2
  }

  command <<<>>>

  output {
    # these two expressions are equivalent
    Float min1 = if value1 < value2 then value1 else value2
    Float min2 = min(value1, value2)
  }
}