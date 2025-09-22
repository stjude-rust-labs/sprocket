#@ except: MetaSections

## This is a test for checking for redundant `= None` assignments in a `input` section.

version 1.2

workflow optionals {
    input {
        # the following are equivalent undefined optional declarations
        String? maybe_five_but_is_not
        String? also_maybe_five_but_is_not = None
    }

    output {
    }
}
