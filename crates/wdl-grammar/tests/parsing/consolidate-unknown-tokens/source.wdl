version 1.3

task foo {
    # Should produce a single `Unknown` token and diagnostic
    ;;;;

    # Same here
    🚀🚀🚀🚀

    # Different unknown characters should also be merged
    ;;;;````
}