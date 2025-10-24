## Test that task variable has correct members in hints section.

version 1.3

task test_hints_scope {
  hints {
    # Pre-evaluation fields are available.
    localization_optional: task.attempt > 3
    maxRetries: if task.name == "test" then 5 else 3
    shortTask: task.id != "" && task.previous.cpu != None
  }

  command <<<
    echo "test"
  >>>
}
