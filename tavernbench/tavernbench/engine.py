"""TavernGym helpers for TavernBench: action indices, game views, combat previews.

Every task starts from a real game advanced by the engine's bots to the first recruit phase,
then reshaped with the engine's debug commands (turn, gold, tier, health, zones, minions); its
state is in `data/<set>/states.jsonl.gz` (`from_state` loads it). Answers can be checked by
executing actions on copies of the game (`apply_ops` applies an option's plays), or with the
engine's combat preview (`preview`, which reseeds every simulated combat).
"""

from __future__ import annotations

import json
from functools import lru_cache

from taverngym import _core

A_END, A_REFRESH, A_FREEZE, A_UPGRADE, A_HERO_POWER = 0, 1, 2, 3, 4
A_BUY, A_SELL, A_ACTIVATE, A_CHOOSE, A_PLAY, A_MAGNETIZE, A_MOVE = 8, 16, 23, 30, 46, 126, 196
N_POS, N_BOARD = 8, 7

ME, OPP = 0, 1


def play_index(hand: int, pos: int) -> int:
    return A_PLAY + hand * N_POS + pos


def move_index(src: int, dst: int) -> int:
    return A_MOVE + src * N_BOARD + dst


@lru_cache(maxsize=1)
def catalog() -> dict:
    return {c["id"]: c for c in json.loads(_core.catalog_json())}


def pool_minions(min_tier=1, max_tier=6):
    """Tavern minions of the current pool (normal versions), sorted by id."""
    out = []
    for c in catalog().values():
        if c["kind"] != "minion" or not c["in_pool"] or not c["implemented"]:
            continue
        if c["id"].endswith("_G") or (c.get("normal_id") and c["normal_id"] != c["id"]):
            continue
        if not (min_tier <= c["tier"] <= max_tier):
            continue
        out.append(c)
    return sorted(out, key=lambda c: c["id"])


def dbg(g, **cmd):
    return json.loads(g.debug(json.dumps(cmd)))


@lru_cache(maxsize=64)
def _base(seed: int):
    g = _core.Game(seed)
    for _ in range(20000):
        if g.turn >= 1 and json.loads(g.view(0))["phase"] == "recruit":
            return g
        for p in range(8):
            if g.needs_action(p):
                g.step(p, g.bot_action(p))
    raise RuntimeError("could not reach the first recruit phase")


# Every scene gives both players the same hero whose power does nothing during the scene
# (Patchwerk: extra Health at the start of the game), so no hidden hero effect changes a label.
NEUTRAL_HERO = "TB_BaconShop_HERO_34"


def scene(seed: int, turn=6, tier=4, gold=10, health=30, players=(ME, OPP),
          clear=("board", "hand", "shop")):
    """A copy of a real game at a recruit phase, reshaped for a task. The other six players
    get a health and tier that fit the turn (they are public in the scene; their warbands
    stay unknown), so the lobby looks like one of that turn."""
    g = _base(seed % 64).copy()
    dbg(g, cmd="set_turn", turn=turn)
    for p in players:
        dbg(g, cmd="set_hero", p=p, hero=NEUTRAL_HERO)
        dbg(g, cmd="set_player", p=p, tier=tier, gold=gold, health=health, armor=0)
        for z in clear:
            dbg(g, cmd="clear_zone", p=p, zone=z)
    h = seed * 2654435761 % (1 << 32)
    for p in range(8):
        if p in players:
            continue
        h = h * 1103515245 + 12345 & 0x7FFFFFFF
        hp = max(1, 40 - turn * 2 - h % 15)
        tr = max(1, min(6, 1 + turn * 10 // 22 + (h >> 8) % 3 - 1))
        dbg(g, cmd="set_player", p=p, tier=tr, health=hp, armor=0)
    return g


def scene_state(g, seen=OPP) -> str:
    """The scene's saved state: the game with player `seen` as the next opponent and its
    current warband as the one scouted in the last combat against it (a player sees only
    warbands it has fought; the text prompt shows the same warband)."""
    st = json.loads(g.state_json())
    me_ = st["players"][ME]
    me_["next_opponent"] = seen
    sc = [{"board": [], "turn": 0} for _ in st["players"]]
    if st["players"][seen]["board"]:
        sc[seen] = {"board": st["players"][seen]["board"], "turn": max(1, st["turn"] - 1)}
    me_["scouted"] = sc
    return json.dumps(st)


def from_state(s: str):
    return _core.Game.from_json(s)


def add(g, p, zone, card, **kw):
    return dbg(g, cmd="add_minion", p=p, zone=zone, card=card, **kw)


def view(g, p=ME) -> dict:
    return json.loads(g.view(p))


def me(g, p=ME) -> dict:
    return view(g, p)["me"]


def legal(g, p=ME) -> list[int]:
    return [i for i, b in enumerate(g.legal_mask(p)) if b]


def preview(g, a=ME, b=OPP, n=4096) -> dict:
    """Win / tie / loss of `a` against `b` over n reseeded combats, and average damage."""
    out = {"win": 0.0, "tie": 0.0, "loss": 0.0, "dmg_taken": 0.0, "dmg_dealt": 0.0, "n": 0}
    left = n
    while left > 0:  # the engine caps one preview at 5000 combats
        k = min(left, 5000)
        r = dbg(g, cmd="combat_preview", a=a, b=b, n=k)
        out["win"] += r["win_a"] * k
        out["loss"] += r["win_b"] * k
        out["tie"] += r["tie"] * k
        out["dmg_dealt"] += r["avg_damage_by_a"] * r["win_a"] * k
        out["dmg_taken"] += r["avg_damage_by_b"] * r["win_b"] * k
        out["n"] += k
        left -= k
    for key in ("win", "tie", "loss", "dmg_taken", "dmg_dealt"):
        out[key] /= out["n"]
    return out


def board_stats(g, p=ME) -> list[list[int]]:
    return [[m["atk"], m["hp"]] for m in me(g, p)["board"]]


def total_stats(g, p=ME) -> int:
    return sum(a + h for a, h in board_stats(g, p))


def apply_ops(g, ops) -> bool:
    """Apply an option's actions, finding each card by its entity id in the current state:
    buy (then play it to the rightmost position when `play`), play (rightmost), sell, and
    spell_target (play hand card `hand` and choose the minion `eid`). False if one fails."""
    for op in ops:
        me_ = me(g)
        k = op["kind"]
        if k == "buy":
            i = next((i for i, m in enumerate(me_["shop"]) if m.get("eid") == op["eid"]), None)
            a = None if i is None else A_BUY + i
        elif k == "play":
            i = next((i for i, m in enumerate(me_["hand"]) if m.get("eid") == op["eid"]), None)
            a = None if i is None else play_index(i, len(me_["board"]))
        elif k == "sell":
            i = next((i for i, m in enumerate(me_["board"]) if m.get("eid") == op["eid"]), None)
            a = None if i is None else A_SELL + i
        elif k == "spell_target":
            a = play_index(op["hand"], 0)
        elif k == "spell":
            i = next((i for i, m in enumerate(me_["hand"]) if m.get("hid") == op["hid"]), None)
            a = None if i is None else play_index(i, 0)  # a spell is played at position 0
        elif k == "play_target":
            i = next((i for i, m in enumerate(me_["hand"]) if m.get("eid") == op["eid"]), None)
            a = None if i is None else play_index(i, len(me_["board"]))
        elif k == "activate_target":
            i = next((i for i, m in enumerate(me_["board"]) if m.get("eid") == op["eid"]), None)
            a = None if i is None else A_ACTIVATE + i
        elif k == "power_target":
            a = A_HERO_POWER + op.get("slot", 0)
        elif k == "magnetize":
            i = next((i for i, m in enumerate(me_["hand"]) if m.get("eid") == op["eid"]), None)
            j = next((j for j, m in enumerate(me_["board"]) if m.get("eid") == op["target"]),
                     None)
            a = None if i is None or j is None else A_MAGNETIZE + i * N_BOARD + j
        elif k == "choose":
            a = A_CHOOSE + op["k"]
        elif k == "play_last":
            a = play_index(len(me_["hand"]) - 1, len(me_["board"])) if me_["hand"] else None
        else:
            raise ValueError(k)
        if a is None or a not in legal(g):
            return False
        g.step(ME, a)
        if k == "buy" and op.get("play"):
            hand = me(g)["hand"]
            h = next((i for i, m in enumerate(hand) if m.get("eid") == op["eid"]), len(hand) - 1)
            a = play_index(h, len(me(g)["board"]))
            if a not in legal(g):
                return False
            g.step(ME, a)
        if k in ("spell_target", "play_target", "power_target", "activate_target"):
            pend = me(g)["pending"]
            tgt = op.get("target", op.get("eid")) if k != "play_target" else op["target"]
            if not pend or tgt not in pend.get("targets", []):
                return False
            g.step(ME, A_CHOOSE + pend["targets"].index(tgt))
        if me(g)["pending"] and not op.get("keep_pending"):
            return False
    return True


def _splitmix(x):
    x = (x + 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
    x = ((x ^ (x >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
    x = ((x ^ (x >> 27)) * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
    return x ^ (x >> 31)


def reseeded(g, salt: int):
    """A copy of the game with new random numbers (the generator state and the combat seed),
    so that what follows is not the future the original game would have."""
    st = json.loads(g.state_json())
    st["rng"]["s"] = [_splitmix(v ^ _splitmix(salt * 4 + i)) for i, v in enumerate(st["rng"]["s"])]
    st["seed"] = _splitmix(st["seed"] ^ salt) % (1 << 63)
    return _core.Game.from_json(json.dumps(st))


def next_turn(g, reseed=None):
    """Play on to this player's next recruit phase: the player ends its turn, the engine's
    bots play the other seats, combat is fought. `reseed` changes the random numbers first
    (to check that an answer does not depend on them). Returns the game (a copy)."""
    if reseed is not None:
        g = reseeded(g, reseed)
    else:
        g = g.copy()
    start = g.turn
    for _ in range(20000):
        if g.turn > start and view(g)["phase"] == "recruit":
            return g
        if g.is_over():
            return None
        for p in range(8):
            if g.needs_action(p):
                if p != ME:
                    g.step(p, g.bot_action(p))
                elif A_END in legal(g, p):
                    g.step(p, A_END)
                else:  # a choice is pending (e.g. a start-of-turn offer): take the first option
                    g.step(p, legal(g, p)[0])
    return None
