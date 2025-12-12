version 1.3

enum LogLevel {
  Debug = "DEBUG",
  Info = "INFO",
  Warning = "WARN",
  Error = "ERROR"
}

workflow enum_inputs {
  input {
    LogLevel level
    LogLevel default_level = LogLevel.Info
  }

  output {
    LogLevel provided_level = level
    LogLevel default_used = default_level
    String level_value = value(level)
    String default_value = value(default_level)
    Boolean is_error = level == LogLevel.Error
  }
}
