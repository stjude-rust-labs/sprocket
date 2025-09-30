version 1.2

#@ except: RequirementsSection
task foo {
    meta {
        description: "more than 140 characters if you include all these letter aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaas"
        outputs: {
            foo: "more than 140 characters if you include all these letter aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaas",
            bar: {
                description: "more than 140 characters if you include all these letter aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaas"
            },
        }
    }

    parameter_meta {
        plain: "more than 140 characters if you include all these letter aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaas"
        nested: {
            description: "more than 140 characters if you include all these letter aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaas"
        }
    }

    input {
        String plain
        String nested
    }

    command <<<>>>

    output {
        Int foo = 42
        Float bar = 21
    }
}
