import json

import pytest

import taverngym


def recruit_game(seed=1):
    g = taverngym.Game(seed, json.dumps({"record_log": True}))
    for p in range(8):
        g.step(p, json.loads(g.view(p))["actions"]["CHOOSE"])  # hero option 0
    for p in range(8):  # choices opened by the heroes (Finley, Denathrius...)
        for _ in range(4):
            v = json.loads(g.view(p))
            if v["me"]["pending"] is None:
                break
            g.step(p, v["actions"]["CHOOSE"])
    return g


def end_turn(g, p):
    """Resolve any pending choice (hero effects like Nguyen / Genn), then end the turn."""
    for _ in range(16):
        v = json.loads(g.view(p))
        if v["me"]["pending"] is None:
            break
        g.step(p, v["actions"]["CHOOSE"])
    if g.needs_action(p):
        g.step(p, 0)


def test_view_structure():
    g = recruit_game()
    v = json.loads(g.view(0))
    assert v["phase"] == "recruit" and v["turn"] == 1
    assert len(v["opponents"]) == 7
    assert len(v["mask"]) == taverngym.N_ACTIONS
    assert v["me"]["shop"] and all("name_zh" in c for c in v["me"]["shop"])


def test_debug_commands_edit_state():
    g = recruit_game()
    dbg = lambda **c: json.loads(g.debug(json.dumps(c)))
    dbg(cmd="set_player", p=0, gold=42, tier=3, health=17)
    dbg(cmd="add_minion", p=0, zone="board", card="BG25_354", golden=True)
    dbg(cmd="add_minion", p=0, zone="hand", card="BG28_300")
    dbg(cmd="edit_minion", p=0, zone="board", index=0, atk=10, hp=11, keywords=["TAUNT"])
    dbg(cmd="edit_minion", p=0, zone="board", index=0, attach="BG36_MidGameEffect_000t73")
    dbg(cmd="give_trinket", p=0, card="BG30_MagicItem_902")
    v = json.loads(g.view(0))["me"]
    assert (v["gold"], v["tier"], v["health"]) == (42, 3, 17)
    m = v["board"][0]
    assert m["golden"] and (m["atk"], m["hp"]) == (15, 16), m  # Fortitude +5/+5 applied
    assert m["keywords"] == ["TAUNT"]
    # (other heroes may hand out cards at turn start, e.g. King Mukla's Bananas)
    assert [h["id"] for h in v["hand"]][-1] == "BG28_300"
    assert v["trinkets"][0]["id"] == "BG30_MagicItem_902"
    with pytest.raises(ValueError):
        g.debug(json.dumps({"cmd": "add_minion", "p": 0, "zone": "board", "card": "NOPE"}))


def test_combat_preview_and_replay_frames():
    g = recruit_game(3)
    g.debug(json.dumps({"cmd": "add_minion", "p": 0, "zone": "board", "card": "BG28_300"}))
    g.debug(json.dumps({"cmd": "add_minion", "p": 1, "zone": "board", "card": "BG36_345"}))
    r = json.loads(g.debug(json.dumps({"cmd": "combat_preview", "a": 0, "b": 1, "n": 50})))
    assert abs(r["win_a"] + r["win_b"] + r["tie"] - 1) < 1e-9
    assert len(r["sample"]["frames"]) >= 2
    # Ending everyone's turn produces combat records with frames.
    for p in range(8):
        end_turn(g, p)
    v = json.loads(g.view(0))
    assert v["turn"] == 2 and v["combats"] and all(c["frames"] for c in v["combats"])


def test_combat_records_describe_only_the_latest_round():
    g = recruit_game(5)
    for turn in range(1, 5):
        for p in range(8):
            if g.needs_action(p):
                end_turn(g, p)
        v = json.loads(g.view(0))
        assert v["turn"] == turn + 1
        turns = {c["turn"] for c in v["combats"]}
        assert turns == {turn}, f"stale combat records: {turns}"
        assert any(0 in (c["a"], c["b"]) for c in v["combats"])


def test_start_of_turn_effects_are_attributed_in_turn_events():
    g = recruit_game(11)
    g.debug(json.dumps({"cmd": "set_hero", "p": 0, "hero": "TB_BaconShop_HERO_35"}))  # Yogg-Saron
    g.debug(json.dumps({"cmd": "set_player", "p": 0, "health": 200}))
    for c in ["BGS_034", "BG36_345", "BG29_888"]:
        g.debug(json.dumps({"cmd": "add_minion", "p": 0, "zone": "board", "card": c}))
    end = json.loads(g.view(0))["actions"]["END"]
    seen = []
    for _ in range(4):
        for p in range(8):
            v = json.loads(g.view(p))
            while v["me"]["pending"]:
                g.step(p, next(i for i in range(v["actions"]["CHOOSE"], v["actions"]["PLAY"]) if v["mask"][i]))
                v = json.loads(g.view(p))
            if g.needs_action(p):
                g.step(p, end)
        seen += json.loads(g.view(0))["me"]["turn_events"]
    yogg = [l for l in seen if "Puzzle Box" in l]
    assert yogg and all("p0 " in l for l in yogg), seen
    # Every recruit-phase stat change names its owner (and its source when known).
    assert all(("p0" in l or "p" in l) for l in seen if "↳" in l)


def test_minion_breakdown_explains_stats_with_sources():
    g = recruit_game(20)
    dbg = lambda **c: g.debug(json.dumps(c))
    dbg(cmd="add_minion", p=0, zone="board", card="BG36_345")
    dbg(cmd="add_spell", p=0, zone="hand", card="BG28_897")  # Tavern Dish Banana +2/+2
    v = json.loads(g.view(0))
    hand = [h["id"] for h in v["me"]["hand"]].index("BG28_897")  # other cards may be in hand
    g.step(0, v["actions"]["PLAY"] + hand * 8)  # cast banana -> target pending
    v = json.loads(g.view(0))
    # (some heroes start with a token on the board: find the Prisonguard by id)
    eid = next(x["eid"] for x in v["me"]["board"] if x["id"] == "BG36_345")
    g.step(0, v["actions"]["CHOOSE"] + v["me"]["pending"]["targets"].index(eid))
    m = next(x for x in json.loads(g.view(0))["me"]["board"] if x["eid"] == eid)
    b = m["breakdown"]
    assert b["base"] == [3, 3] and (m["atk"], m["hp"]) == (5, 5)
    assert [(x["src"]["id"], x["atk"], x["hp"]) for x in b["mods"]] == [("BG28_897", 2, 2)]
    assert b["other"] == [0, 0]


def test_view_shows_patch_state_and_subsystem_commands():
    g = recruit_game(3)
    dbg = lambda **c: json.loads(g.debug(json.dumps(c)))
    v = json.loads(g.view(0))
    assert v["version"] == taverngym.game_version()
    assert v["has_aberrations"] == ("ABERRATION" in v["lobby_races"])
    assert (v["me"]["deity"] is not None) == v["has_aberrations"]
    assert all({"frozen", "kept", "ticket", "price"} <= set(c) for c in v["me"]["shop"])
    assert v["me"]["hand_queue"] == [] and "recent_opponents" in v["me"]
    # Quests, Rewards, Secrets and Hero Powers can be set up from the debug panel.
    dbg(cmd="give_quest", p=0, quest="BG24_Quest_123", reward="BG24_Reward_107")
    dbg(cmd="give_reward", p=0, card="BG24_Reward_109")
    dbg(cmd="add_secret", p=0, card="TB_Bacon_Secrets_01")
    dbg(cmd="set_hero_power", p=0, slot=1, card="TB_BaconShop_HP_101")
    me = json.loads(g.view(0))["me"]
    assert me["quests"][0]["quest"]["id"] == "BG24_Quest_123"
    assert me["quests"][0]["reward"]["id"] == "BG24_Reward_107"
    assert [r["id"] for r in me["rewards"]] == ["BG24_Reward_109"]
    assert [x["id"] for x in me["secrets"]] == ["TB_Bacon_Secrets_01"]
    assert me["hero_powers"][1]["id"] == "TB_BaconShop_HP_101"
    for bad in (
        {"cmd": "give_quest", "p": 0, "quest": "BG36_345"},
        {"cmd": "add_secret", "p": 0, "card": "BG36_345"},
        {"cmd": "set_hero_power", "p": 0, "slot": 0, "card": "BG36_345"},
    ):
        with pytest.raises(ValueError):
            g.debug(json.dumps(bad))
    # Freezing shows on the cards; kept cards are marked on the next turn.
    g.step(0, json.loads(g.view(0))["actions"]["FREEZE"])
    assert all(c["frozen"] for c in json.loads(g.view(0))["me"]["shop"])
    # A trinket offer shows the price for this player and the offer weight.
    dbg(cmd="offer_trinkets", p=0, greater=False)
    pend = json.loads(g.view(0))["me"]["pending"]
    assert pend["kind"] == "trinket"
    assert all({"cost", "base_cost", "weight"} <= set(o) for o in pend["options"])
