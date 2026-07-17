version 1.2

# The deprecated `SnakeCase` alias maps to `NamingConvention`. It should emit
# only a migration note from `KnownRules`, and must NOT trigger a
# `MeaninglessLintDirective` warning even though `NamingConvention` is not an
# analysis rule.
#@ except: SnakeCase
task BadName {
    command <<<>>>

    output {
        Int result = 0
    }
}
