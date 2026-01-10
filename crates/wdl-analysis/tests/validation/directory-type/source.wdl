## This is a test of using the `Directory` type from WDL 1.2.
## This test should not have any diagnostics.

version 1.3

workflow test {
    Directory x = "foo"
}
