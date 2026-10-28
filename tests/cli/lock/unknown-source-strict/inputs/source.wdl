version 1.3

task unknown_image {
    command <<<>>>

    requirements {
        # We can't do anything with unknown images
        container: "unknown://foo"
    }
}
