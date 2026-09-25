# Global exceptions should be allowed
#@ except: UsingFallbackVersion

version 1.3

# Shouldn't collide with `ExceptDirectiveValid`
#@ except: UnusedCall
task do_work {
    command <<<>>>

    output {
        Int result = 0
    }
}

# Shouldn't collide with `KnownRules`
#@ except: WhatIsThisMysteriousRule
workflow calculate {
    # Unnecessary, the result is used
    #@ except: UnusedCall
    call do_work

    #@ except: UnusedCall
    call do_work as do_work2

    # Should work with multiple rules
    #@ except: UnnecessaryFunctionCall, UnusedDeclaration
    Boolean exists = defined("hello")

    # Unnecessary, the declaration is used
    #@ except: UnnecessaryFunctionCall, UnusedDeclaration
    Boolean exists2 = true

    output {
        # We're using the results here!
        Int result = do_work.result
        Boolean result_exists = exists2
    }
}

# It can also be suppressed at any level
#@ except: MeaninglessLintDirective
task suppressed {
    input {
        #@ except: UnusedInput
        String definitely_used
    }

    command <<<
        echo "~{definitely_used}"
    >>>
}

task suppressed2 {
    #@ except: MeaninglessLintDirective
    input {
        #@ except: UnusedInput
        String definitely_used
    }

    command <<<
        echo "~{definitely_used}"
    >>>
}