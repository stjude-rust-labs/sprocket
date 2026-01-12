version 1.3

task test_max {
  input {
    Int value1
    Float value2
  }

  command <<<>>>

  output {
    # these two expressions are equivalent
    Float max1 = if value1 > value2 then value1 else value2
    Float max2 = max(value1, value2)
  }
}
