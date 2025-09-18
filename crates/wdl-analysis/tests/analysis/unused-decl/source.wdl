## This is a test of unused inputs and declarations.

version 1.1

task foo {
    input {
        # Unused input
        Int unused_input = 0
        # Unused but excepted
        #@ except: UnusedInput
        Int unused_input2 = 0
        Int used = 1
        Int x = used + 5
        String y = "~{x}"
    }

    # Unused private decl
    Int unused_decl = 0
    # Unused but excepted
    #@ except: UnusedDeclaration
    Int unused_decl2 = 0
    Int used_decl = 1
    Int x_decl = used_decl + 5
    String y_decl = "~{x_decl}"

    command <<<>>>

    output {
        String o1 = y
        String o2 = y_decl
    }
}

workflow test {
    input {
        # Unused input
        Int unused_input = 0
        # Unused but excepted
        #@ except: UnusedInput
        Int unused_input2 = 0
        Int used = 1
        Int x = used + 5
        String y = "~{x}"
    }

    # Unused private decl
    Int unused_decl = 0
    # Unused but excepted
    #@ except: UnusedDeclaration
    Int unused_decl2 = 0
    Int used_decl = 1
    Int x_decl = used_decl + 5
    String y_decl = "~{x_decl}"

    output {
        String o1 = y
        String o2 = y_decl
    }
}
