version 1.4

task empty_report {
  command <<<
  touch report.txt
  >>>

  output {
    File report = "report.txt"

    validation {
      size(report) > 0: "Report must not be empty",
    }
  }
}

workflow task_output_validation_fail {
  call empty_report
}
