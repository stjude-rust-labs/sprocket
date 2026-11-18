version 1.3

task test {
  meta {
    import: "abc"
    None: "xyz"
    enum: "e"
    nested: {
      if: true,
    }
  }

  command <<<
    echo "~{task.meta.import}" "~{task.meta.None}" "~{task.meta.enum}" "~{task.meta.nested.if}"
  >>>

  output {
    String import_value = task.meta.import
    String none_value = task.meta.None
    String enum_value = task.meta.enum
    Boolean if_value = task.meta.nested.if
    Object o = object {
      import: 1,
      enum: 2,
      None: 3,
    }
    Int sum = o.import + o.enum + o.None
  }
}
