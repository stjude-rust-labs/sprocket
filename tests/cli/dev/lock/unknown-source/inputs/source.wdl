version 1.3

task unknown_image {
    command <<<>>>

    requirements {
        # We can't do anything with unknown images
        container: "unknown://foo"
    }
}

task partial_array {
    command <<<>>>

    requirements {
        # But a single unknown image shouldn't invalidate the entire array
        container: ["unknown://foo", "python:3"]
    }
}
