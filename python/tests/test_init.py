import sprocket_bio


def test_has_package_docs() -> None:
    assert sprocket_bio.__doc__, "sprocket_bio must have a module docstring"
