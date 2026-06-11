from sprocket_bio.diagnostics import Mode


def test_mode_eq():
    assert Mode.FULL == Mode.FULL
    assert Mode.ONE_LINE == Mode.ONE_LINE
    assert Mode.FULL != Mode.ONE_LINE

    assert Mode.FULL is Mode.FULL
    assert Mode.ONE_LINE is Mode.ONE_LINE
    assert Mode.FULL is not Mode.ONE_LINE
