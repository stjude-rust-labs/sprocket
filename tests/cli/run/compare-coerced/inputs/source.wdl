## This is a representative WDL workflow to test the behavior of the run command


version 1.3

task compare_coerced {
  Array[Int] i = [1, 2, 3]
  Array[Float] f1 = i
  Array[Float] f2 = [1.0, 2.0, 3.0]

  command <<<>>>

  output {
    # Ints are automatically coerced to Floats for comparison
    Boolean is_true = f1 == f2
  }
}