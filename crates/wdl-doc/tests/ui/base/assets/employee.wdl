version 1.2

# Documented with comments
## A base person struct.
##
## This defines a named person.
struct Person {
    ## The name of the person.
    ##
    ## This is the full (first, last) name of the person.
    String name
}

# Documented with meta sections
struct Employee {
    Person person
    Int id

    meta {
        description: "An `Employee` is a `Person` who is formally employed by the organization."
    }

    parameter_meta {
        person: "The person from which the employee is derived."
        id: "The employee ID number."
    }
}

## A contractor record used for non-employee workers.
##
## Contractors represent individuals who provide services for a limited period of time and are not
## formally employed by the organization.
struct Contractor {
    ## The person from which the contractor is derived.
    Person person

    ## A temporary contractor identifier.
    Int id

    ## The organization the contractor is associated with.
    String vendor

    ## The date on which the contract begins (ISO-8601).
    String start_date

    ## The date on which the contract ends (ISO-8601).
    String end_date

    meta {
        description: "This description should not appear, doc comments take precedence."
    }

    parameter_meta {
        person: "This description should not appear, doc comments take precedence."
        id: "This description should not appear, doc comments take precedence."
        contractor_id: "This description should not appear, doc comments take precedence."
        vendor: "This description should not appear, doc comments take precedence."
        start_date: "This description should not appear, doc comments take precedence."
        end_date: "This description should not appear, doc comments take precedence."
    }
}

workflow employee_is_person {
    meta {
        description: "Determines whether the given person matches the given employee."
        outputs: {
            result: "Whether the given person matches the given employee."
        }
    }

    input {
        Employee employee
        Person person
    }

    output {
        Boolean result = employee.person == person
    }
}

task contractor_is_person {
    meta {
        description: "Determines whether the given person matches the given contractor."
        outputs: {
            result: "Whether the given person matches the given contractor."
        }
    }

    input {
        Contractor contractor
        Person person
    }

    command { }

    output {
        Boolean result = contractor.person == person
    }
}