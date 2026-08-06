import pytest

from sprocket_bio.grammar.version import V1, SupportedVersion


def test_v1_ord() -> None:
    assert V1.ZERO < V1.ONE < V1.TWO < V1.THREE < V1.FOUR


def test_supported_version_new() -> None:
    v = SupportedVersion.V1(V1.TWO)
    assert v._0 == V1.TWO


def test_supported_version_match() -> None:
    v = SupportedVersion.V1(V1.FOUR)

    match v:
        case SupportedVersion.V1(V1.ONE):
            pytest.fail("unreachable")
        case SupportedVersion.V1(V1.FOUR):
            # Success
            pass
        case _:
            pytest.fail("unreachable")


def test_supported_version_has_same_major_version() -> None:
    assert SupportedVersion.V1(V1.ZERO).has_same_major_version(
        SupportedVersion.V1(V1.TWO)
    )


def test_supported_version_ord() -> None:
    assert SupportedVersion.V1(V1.ZERO) < SupportedVersion.V1(V1.ONE)
