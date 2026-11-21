version 1.4

task foo {
    # Should produce a single `Unknown` token and diagnostic
    ;;;;

    # Same here
    🚀🚀🚀🚀

    # Different unknown characters should also be merged
    ;;;;````

    # But valid tokens can immediately follow
    ;`input {}`;
}