"""Rule cases through the Python API: framework effects, single cards, and saving and
loading a game."""

import json

import pytest

import taverngym
from taverngym import _core

LOBBY = ["MURLOC", "ELEMENTAL", "DEMON", "PIRATE", "ABERRATION"]


def snap(g):
    return json.loads(g.state_json())


def restore(s):
    return taverngym.Game.from_json(json.dumps(s))


def view(g):
    return json.loads(g.view(0))


def dbg(g, **kw):
    return json.loads(g.debug(json.dumps(kw)))


def fixture(seed=541, n=2, **cfg):
    """Heroes picked, then every seat emptied: no cards, powers or effects, 30 Gold, Tier 6."""
    cfg = dict(record_log=True, num_players=n, triple_copies=99, lobby_types=LOBBY,
               lesser_trinket_turn=0, greater_trinket_turn=0, **cfg)
    g = taverngym.Game(seed, json.dumps(cfg))
    for p in range(n):
        g.step(p, json.loads(g.view(p))["actions"]["CHOOSE"])
    s = snap(g)
    for p in s["players"]:
        for k in ["board", "hand", "pending", "effects", "quests", "rewards", "trinkets",
                  "powers", "turn_data", "counters", "secrets"]:
            p[k] = []
        p.update(skipping=False, gold=30, tier=6)
        for k, v in p["stats"].items():
            if isinstance(v, bool):
                p["stats"][k] = False
            elif isinstance(v, int):
                p["stats"][k] = 0
            elif isinstance(v, list):
                p["stats"][k] = []
    return restore(s)


def add(g, card, zone="board", p=0, **kw):
    dbg(g, cmd="add_minion", p=p, zone=zone, card=card, **kw)


def choose_target(g, eid):
    v = view(g)
    g.step(0, v["actions"]["CHOOSE"] + v["me"]["pending"]["targets"].index(eid))


def hero_power(g, card, target=None):
    dbg(g, cmd="set_hero_power", p=0, slot=0, card=card)
    g.step(0, view(g)["actions"]["HERO_POWER"])
    if target is not None:
        choose_target(g, target)


@pytest.mark.parametrize("n,last", [(2, False), (2, True), (8, False), (8, True)])
def test_an_end_of_turn_lethal_is_eliminated_before_combat(n, last):
    # 1 Health: Imposing Percussionist + Young Murk-Eye's end-of-turn Battlecry kills it.
    g = fixture(n=n)
    dbg(g, cmd="set_turn", turn=12)
    pair = next(x for x in snap(g)["pairs"] if 0 in x)
    opponent = pair[1] if pair[0] == 0 else pair[0]
    add(g, "BG26_525")
    add(g, "BG22_403")
    add(g, "BG36_345", p=opponent, atk=1, hp=1)
    for p in (0, opponent):
        dbg(g, cmd="set_player", p=p, health=1, armor=0)
    if last:
        for p in range(1, n):
            g.step(p, 0)
    g.step(0, 0)
    if not last:
        for p in range(1, n):
            if g.needs_action(p):
                g.step(p, 0)
    after = snap(g)
    me = after["players"][0]
    assert not me["alive"] and me["placement"] == n, (last, me)
    assert all(0 not in (c["a"], c["b"]) for c in after["last_combats"])


def test_a_queued_triple_waits_for_an_open_battlecry_menu():
    # Two Sellementals, a full hand with Faceless Manipulator; Reflective Pendant has a
    # third Sellemental waiting for room.
    g = fixture(n=8)
    s = snap(g)
    s["cfg"]["triple_copies"] = 3
    g = restore(s)
    add(g, "BGS_115")
    add(g, "BGS_115")
    add(g, "BG_EX1_564", "hand")
    for _ in range(9):
        dbg(g, cmd="add_spell", p=0, zone="hand", card="BG28_810")
    dbg(g, cmd="give_trinket", p=0, card="BG30_MagicItem_706")
    dbg(g, cmd="clear_zone", p=0, zone="shop")
    g.step(0, view(g)["actions"]["PLAY"] + 2)
    v = view(g)
    targets = v["me"]["pending"]["targets"]
    board = {m["eid"] for m in snap(g)["players"][0]["board"]}
    assert set(targets) <= board, "a target was tripled away under the menu"
    g.step(0, v["actions"]["CHOOSE"])
    assert "BG_EX1_564" not in [x["id"] for x in view(g)["me"]["board"]], "Faceless copied"


@pytest.mark.parametrize("survivors", [3, 4])
def test_a_deathrattle_triple_frees_no_room_for_reborn(survivors):
    # Dead Bellringer destroys Cadaver Caretaker: three Skeletons, then its Reborn.
    g = fixture()
    s = snap(g)
    s["cfg"]["triple_copies"] = 3
    g = restore(s)
    cards = ["BG36_511", "BG30_125", "BGS_039", "BG21_015"] + ["BG35_155"] * (survivors == 4)
    for card in cards:
        add(g, card)
    victim = snap(g)["players"][0]["board"][1]["eid"]
    g.step(0, view(g)["actions"]["ACTIVATE"])
    if view(g)["me"]["pending"]:
        choose_target(g, victim)
    board = [x["id"] for x in view(g)["me"]["board"]]
    assert board.count("BG30_125") == (survivors == 3), board


@pytest.mark.parametrize("extra", [False, True])
def test_the_newest_minion_counts_avenge(extra):
    # Lurker (Avenge (1): +1/+1) played last: it is the newest entity unless a Coin follows.
    g = fixture()
    add(g, "BGS_039", p=1, atk=10, hp=100)
    add(g, "BGS_039", atk=1, hp=1)
    add(g, "BG31_HERO_811t7", zone="hand", atk=0, hp=100)
    g.step(0, view(g)["actions"]["PLAY"] + 1)
    if extra:
        dbg(g, cmd="add_spell", p=0, zone="hand", card="BG28_810")
    s = snap(g)
    s["cfg"]["max_combat_attacks"] = 1
    g = restore(s)
    eid = s["players"][0]["board"][1]["eid"]
    frames = dbg(g, cmd="combat_preview", a=0, b=1, n=1)["sample"]["frames"]
    final = next(m for m in frames[-1]["boards"][0] if m["eid"] == eid)
    assert (final["atk"], final["hp"]) == (1, 101)


def test_fixed_stats_keep_the_eternal_knight_bonus():
    # Pack Tactics' 3/3 copy of an Eternal Knight after three Knight deaths.
    g = fixture()
    s = snap(g)
    s["players"][0]["stats"]["eternal_knight_deaths"] = 3
    g = restore(s)
    add(g, "BG25_008")
    add(g, "BGS_039", p=1, atk=1, hp=1000)
    dbg(g, cmd="add_secret", p=0, card="TB_Bacon_Secrets_15")
    first = view(g)["me"]["board"][0]["eid"]
    r = dbg(g, cmd="combat_preview", a=0, b=1, n=1, after_start=True, first_attacker=1)
    copy = next(m for f in r["sample"]["frames"] for m in f["boards"][0]
                if m["id"] == "BG25_008" and m["eid"] != first)
    assert (copy["atk"], copy["max_hp"]) == (15, 9)
    # Xyrella's "set its stats to 2" on a Tavern Knight, with the Undead bonus too.
    g = fixture()
    s = snap(g)
    s["players"][0]["stats"].update(eternal_knight_deaths=2, undead_atk=5, undead_hp=7)
    g = restore(s)
    add(g, "BG25_008", zone="shop")
    hero_power(g, "BG20_HERO_101p", view(g)["me"]["shop"][-1]["sid"])
    m = snap(g)["players"][0]["hand"][0]["item"]["Minion"]
    assert (m["atk"], m["max_hp"]) == (15, 13)


def test_devour_spits_onto_a_random_minion_without_a_menu():
    g = fixture()
    for card in ["BGS_039", "BG21_015", "BG36_345"]:
        add(g, card)
    hero_power(g, "BG20_HERO_301p", view(g)["me"]["board"][0]["eid"])
    assert view(g)["me"]["pending"] is None


def test_etc_never_offers_hero_power_bound_buddies():
    bound = {"TB_BaconShop_HERO_21_Buddy", "TB_BaconShop_HERO_62_Buddy"}
    for seed in range(150):
        g = fixture(seed)
        hero_power(g, "BG25_HERO_105p")
        ids = {x["id"] for x in view(g)["me"]["pending"]["options"]}
        assert not ids & bound, (seed, ids)


def test_a_capped_gain_feeds_no_listener():
    # A Blood Gem on a minion at the Health limit gives nothing: Titanic Guardian gets none.
    g = fixture()
    add(g, "BGS_039", hp=2_147_483_647)
    add(g, "TB_BaconShop_HERO_39_Buddy")
    target = view(g)["me"]["board"][0]["eid"]
    before = [m["hp"] for m in view(g)["me"]["board"]]
    dbg(g, cmd="add_spell", p=0, zone="hand", card="BG20_GEM")
    v = view(g)
    g.step(0, v["actions"]["PLAY"] + (len(v["me"]["hand"]) - 1) * 8)
    choose_target(g, target)
    assert [m["hp"] for m in view(g)["me"]["board"]] == before


def test_ancient_mask_saturates_a_capped_deity():
    g = fixture()
    dbg(g, cmd="set_deity", p=0, atk=2_147_483_647, hp=2_147_483_647)
    dbg(g, cmd="give_trinket", p=0, card="BG36_MagicItem_602")
    d = snap(g)["players"][0]["deity"]
    assert (d["atk"], d["hp"], d["golden"]) == (2_147_483_647, 2_147_483_647, True)


def test_batch_load_refuses_another_card_table():
    v = taverngym.BatchEnv(1)
    saved = v.save(0)
    slot = json.loads(saved)
    slot["game"]["card_table"] ^= 1
    with pytest.raises(ValueError, match="card table"):
        v.load(0, json.dumps(slot))
    # The running game is untouched, and a good save still loads.
    assert v.save(0) == saved
    v.load(0, saved)


def test_the_catalog_has_the_cards_used_here():
    ids = {r["id"] for r in json.loads(_core.catalog_json())}
    assert {"BG26_525", "BG22_403", "BGS_115", "BG_EX1_564", "BG30_MagicItem_706",
            "BG36_511", "BG30_125", "BG31_HERO_811t7", "BG25_008", "TB_Bacon_Secrets_15",
            "BG20_HERO_101p", "BG20_HERO_301p", "BG25_HERO_105p", "BG36_MagicItem_602"} <= ids
