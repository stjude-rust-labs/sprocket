## This is a test of long call target names.
## Technically, this is an extension of the grammar that is expected
## to be treated as an error when call statements are resolved; however
## we made the grammar more permissable so we can report this error gracefully.

version 1.3

workflow test {
    call foo.bar.baz.qux
    call foo.bar.baz { input: }
    call foo.bar.baz.qux as foo
}
