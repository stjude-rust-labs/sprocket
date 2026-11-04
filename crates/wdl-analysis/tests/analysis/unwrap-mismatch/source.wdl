## This is a test of type checking the unwrap operator

version 1.4

struct Income {
    Int amount
}

struct Person {
    String name
    Income? income
}

workflow test {
    input {
        Int? maybe_five = 5
        Array[Int?] maybes = [1, None]
        Person person = Person { name: "Fred", income: Income { amount: 10 } }
        String? name
    }

    # Valid unwraps
    Int five = maybe_five!
    Int fifteen = maybe_five! + 10
    Int neg = -maybe_five!
    Int first = maybes[0]!
    Int amount = person.income!.amount
    Int first_or = select_first([maybes[1]])!
    Boolean eq = maybe_five! == 5
    String greeting = "~{'hello ' + name!}"

    # Invalid unwraps of non-optional types
    Int bad = five!
    String bad_string = "hello"!
    Int bad_double = maybe_five!!
    Income bad_member = person.income!.amount!

    # Unwrapping `None` is allowed by the type checker
    Int? none = None!

    # Assigning an unwrapped value to an incompatible type
    String wrong = maybe_five!
}
