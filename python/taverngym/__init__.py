"""TavernGym: a Hearthstone Battlegrounds (36.6.3.253932) solo-mode simulator with a
reinforcement-learning interface.

- `Game`: one eight-player game, every seat driven explicitly.
- `BattlegroundsEnv`: one learning seat against seven scripted bots (Gymnasium-style).
- `BatchEnv`: many games stepped together, any set of controlled seats.
"""

from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

try:
    from taverngym._core import (
        N_ACTIONS,
        N_CARDS,
        OBS_CARDS,
        OBS_FLOATS,
        Game,
        SoloEnv,
        action_name,
        card_info,
        card_static,
        catalog_json,
        default_config_json,
        game_version,
        layout,
        run_bot_games,
        server_rules_version,
        version,
    )
except ImportError as exc:  # pragma: no cover - only hit before the Rust build
    raise ImportError("taverngym._core is not built; run `pip install .` first") from exc


def build_info():
    """How the loaded native extension was built: `source_hash` (the fingerprint of the
    engine and binding sources it was compiled from), `profile` and the file loaded."""
    from taverngym import _core

    source, profile = _core.build_info()
    return {"source_hash": source, "profile": profile, "file": _core.__file__}


def source_hash(repo_root=REPO_ROOT):
    """The `source_hash` an extension built from this checkout has (`crates/taverngym-py/build.rs`
    computes the same), or None where the sources are not there (an installed package)."""
    import zlib

    crates = Path(repo_root) / "crates"
    data = crates / "taverngym" / "data" / "cards.json"
    if not data.is_file():
        return None
    files = [*(crates / "taverngym" / "src").rglob("*.rs"),
             *(crates / "taverngym-py" / "src").rglob("*.rs"), data]  # fmt: skip
    h = 0xCBF29CE484222325
    for rel, f in sorted((f.relative_to(crates).as_posix(), f) for f in files):
        crc = zlib.crc32(f.read_bytes().replace(b"\r", b"")) & 0xFFFFFFFF
        for b in rel.encode() + b"\0" + crc.to_bytes(4, "little"):
            h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def build_is_current(repo_root=REPO_ROOT):
    """Whether the loaded extension was built from the sources of this checkout (None when
    that cannot be told: no sources here)."""
    built, here = build_info()["source_hash"], source_hash(repo_root)
    return None if here is None else built == here


from taverngym.batch import BatchEnv  # noqa: E402
from taverngym.env import BattlegroundsEnv  # noqa: E402

__all__ = [
    "N_ACTIONS",
    "N_CARDS",
    "OBS_CARDS",
    "OBS_FLOATS",
    "REPO_ROOT",
    "BatchEnv",
    "BattlegroundsEnv",
    "Game",
    "SoloEnv",
    "action_name",
    "build_info",
    "build_is_current",
    "card_info",
    "card_static",
    "catalog_json",
    "default_config_json",
    "game_version",
    "layout",
    "run_bot_games",
    "server_rules_version",
    "source_hash",
    "version",
]
