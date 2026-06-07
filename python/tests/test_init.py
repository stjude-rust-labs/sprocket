import sprocket_bio


def test_has_package_docs():
    assert len(sprocket_bio.__doc__) > 0
