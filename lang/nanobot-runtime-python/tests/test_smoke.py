"""W2.T1 smoke tests — package importable + version pinned."""


def test_package_importable() -> None:
    import nanobot_runtime

    assert nanobot_runtime.__version__ == "0.1.0"


def test_main_entry_exists() -> None:
    from nanobot_runtime.__main__ import main

    assert callable(main)
