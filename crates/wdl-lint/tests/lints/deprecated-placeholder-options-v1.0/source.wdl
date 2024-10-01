## This is a test of the `DeprecatedPlaceholderOption` lint.

version 1.0

# None of these lints should trigger as the version is WDL v1.0 (prior to
# placeholder options being deprecated).
task a_task {
    #@ except: DescriptionMissing
    meta {}

    String bad_sep_option = "~{sep="," numbers}"
    String bad_true_false_option = "~{true="--enable-foo" false="" allow_foo}"
    String bad_default_option = "~{default="false" bar}"

    command <<<
        python script.py ~{sep=" " numbers}
        example-command ~{true="--enable-foo" false="" allow_foo}
        another-command ~{default="foobar" bar}
    >>>

    output {}

    #@ except: RuntimeSectionKeys
    runtime {}
}
