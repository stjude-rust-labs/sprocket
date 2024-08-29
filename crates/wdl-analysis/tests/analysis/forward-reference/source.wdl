## This is a test for forward references in a WDL task.

version 1.1

task forward_reference {
    # OK as the forward reference is to a string
    String x = a

    input {
        String a = "hello"

        # NOT OK as the forward reference is not to an Int
        Int y = z
    }

    String z = "5"

    command <<<>>>
}
