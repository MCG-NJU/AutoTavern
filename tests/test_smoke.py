import json

import taverngym


def test_versions_match_card_data():
    # The card table uses the pinned 37.0.2 client data and server overlay
    # (tools/overlays/37.0.2.json).
    root = taverngym.REPO_ROOT
    meta = json.loads((root / "tools" / "overlays" / "37.0.2.json").read_text())["meta"]
    assert taverngym.game_version() == meta["client_version"] == "37.0.2.256589"
    assert taverngym.server_rules_version() == meta["server_patch"]
    assert taverngym.version()


def test_the_extension_records_the_sources_it_was_built_from():
    # The card data does not change when the engine's code does: the source hash tells a
    # stale binary from a current one.
    info = taverngym.build_info()
    assert info["file"] and info["profile"] in ("release", "debug")
    assert len(info["source_hash"]) == 16
    # Built from this checkout before the tests run.
    assert info["source_hash"] == taverngym.source_hash()
    assert taverngym.build_is_current() is True
    # A copied python/ tree has no sources to compare with.
    assert taverngym.source_hash("/nonexistent") is None
    assert taverngym.build_is_current("/nonexistent") is None
