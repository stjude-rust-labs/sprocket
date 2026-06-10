## This is a test for forward references in a WDL task.

#@ except: UnusedDeclaration

version 1.3

task forward_reference {
    input {
        String a = "hello"

        # NOT OK as the forward reference is not to an Int
        Int y = z
    }

    # OK as the forward reference is to a string
    String x = a

    String z = "5"

    command <<<>>>

    requirements {
        cpu: y
    }

    hints {
        max_cpu: y
    }
}
