#@ except: UnusedKall, ThisIsNotARealRule, UnusedInput

version 1.1

# Make sure trailing commas don't trip us up: <https://github.com/stjude-rust-labs/sprocket/issues/925>
#@ except: MetaSections,
workflow test {
    input {
        String message = "Hello, World!"
    }

    output {
        String out = message
    }
}
