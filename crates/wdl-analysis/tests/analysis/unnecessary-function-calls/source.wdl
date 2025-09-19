## This is a test of unnecessary function calls.
#@ except: UnusedDeclaration

version 1.1

workflow test {
    # NOT OK
    String foo = select_first(['foo', 'bar', 'baz'])

    # NOT OK
    Array[String] bar = select_all(['foo', 'bar', 'baz'])

    # NOT OK
    Boolean baz = defined(['foo', 'bar', 'baz'])

    # OK
    String qux = select_first(['foo', None, 'baz'])

    # OK
    Array[String] quux = select_all(['foo', None, 'baz'])

    # OK
    File? file = None
    Boolean quuux = defined(file)
}
