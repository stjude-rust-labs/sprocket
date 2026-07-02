from sprocket_bio.grammar.version import SupportedVersion, V1


def test_v1_ord() -> None:
    assert V1.ZERO < V1.ONE < V1.TWO < V1.THREE < V1.FOUR
