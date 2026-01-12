version 1.3

workflow optionals {
  input {
    Int certainly_five = 5      # an non-optional declaration
    Int? maybe_five_and_is = 5  # a defined optional declaration

    # the following are equivalent undefined optional declarations
    String? maybe_five_but_is_not
    String? also_maybe_five_but_is_not = None
  }

  output {
    Boolean test_defined = defined(maybe_five_but_is_not) # Evaluates to false
    Boolean test_defined2 = defined(maybe_five_and_is)    # Evaluates to true
    Boolean test_is_none = maybe_five_but_is_not == None  # Evaluates to true
    Boolean test_not_none = maybe_five_but_is_not != None # Evaluates to false
    Boolean test_non_equal = maybe_five_but_is_not == also_maybe_five_but_is_not
  }
}