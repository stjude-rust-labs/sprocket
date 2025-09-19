version 1.2

task outputs {
  input {
    Int t
  }

  command <<<
  printf ~{t} > threshold.txt
  touch a.csv b.csv
  >>>

  output {
    Int threshold = read_int("threshold.txt")
    Array[File]+ csvs = glob("*.csv")
    Boolean two_csvs = length(csvs) == 2
  }
}