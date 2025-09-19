#@ except: MetaDescription, ExpectedRuntimeKeys, ShellCheck

## This is a test of the `DeprecatedPlaceholder` lint.

version 1.1

task a_failing_task {
    meta {}

    Array[String] numbers = ["1", "2", "3"]
    Boolean allow_foo = true
    String bar = "bar"

    String bad_sep_option = "~{sep="," numbers}"
    String bad_true_false_option = "~{true="--enable-foo" false="" allow_foo}"
    String bad_default_option = "~{default="false" bar}"

    command <<<
        python script.py ~{sep=" " numbers}
        example-command ~{true="--enable-foo" false="" allow_foo}
        another-command ~{default="foobar" bar}
    >>>

    output {}

    runtime {}
}

task a_better_task {
    meta {}

    Array[String] numbers = ["1", "2", "3"]
    Boolean allow_foo = true
    String bar = "bar"

    String good_sep_option = "~{sep(",", numbers)}"
    String good_true_false_option = "~{if allow_foo then "--enable-foo" else ""}"
    String good_default_option = "~{select_first([bar, "false"])}"

    command <<<
        python script.py ~{sep(" ", numbers)}
        example-command ~{if allow_foo then "--enable-foo" else ""}
        another-command ~{select_first([bar, "foobar"])}
        # OR also equivalent
        another-command ~{if defined(bar) then bar else "foobar"}
    >>>

    output {}

    runtime {}
}

#@ except: DeprecatedPlaceholder
task an_ignored_task {
    meta {}

    Array[String] numbers = ["1", "2", "3"]
    Boolean allow_foo = true
    String bar = "bar"

    String bad_sep_option = "~{sep="," numbers}"
    String bad_true_false_option = "~{true="--enable-foo" false="" allow_foo}"
    String bad_default_option = "~{default="false" bar}"

    command <<<
        python script.py ~{sep=" " numbers}
        example-command ~{true="--enable-foo" false="" allow_foo}
        another-command ~{default="foobar" bar}
    >>>

    output {}

    runtime {}
}
