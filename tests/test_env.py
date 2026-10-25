import json

import numpy as np

import taverngym


def test_constants():
    assert taverngym.N_ACTIONS == 245
    assert taverngym.OBS_FLOATS > 0 and taverngym.OBS_CARDS > 0
    assert taverngym.card_info(1)[0]
    assert taverngym.action_name(0) == "EndTurn"


def test_random_agent_episode_completes():
    env = taverngym.BattlegroundsEnv(seed=1)
    obs, _ = env.reset(seed=1)
    rng = np.random.default_rng(0)
    assert obs["features"].shape == (taverngym.OBS_FLOATS,)
    assert obs["cards"].shape == (taverngym.OBS_CARDS,)
    done, steps, reward = False, 0, 0.0
    while not done and steps < 5000:
        legal = np.flatnonzero(obs["action_mask"])
        assert legal.size > 0
        # Prefer ending the turn sometimes so games progress.
        a = 0 if (legal[0] == 0 and rng.random() < 0.3) else int(rng.choice(legal))
        obs, reward, done, _, info = env.step(a)
        assert info["illegal"] is None
        steps += 1
    assert done
    assert 1 <= info["placement"] <= 8
    assert -1.0 <= reward <= 1.0


def test_illegal_action_is_rejected():
    env = taverngym.BattlegroundsEnv(seed=2)
    obs, _ = env.reset(seed=2)
    illegal = int(np.flatnonzero(~obs["action_mask"])[0])
    _, r, done, _, info = env.step(illegal)
    assert info["illegal"] is not None and r < 0 and not done


def test_full_game_selfplay_with_bots_and_snapshot():
    g = taverngym.Game(5)
    while not g.is_over():
        for p in range(g.num_players):
            if g.needs_action(p):
                g.bot_play(p)
    assert sorted(g.placements()) == list(range(1, 9))
    snap = g.state_json()
    g2 = taverngym.Game.from_json(snap)
    assert g2.state_json() == snap


def test_config_override_and_batch_runner():
    cfg = json.loads(taverngym.default_config_json())
    assert cfg["board_size"] == 7
    turns = taverngym.run_bot_games(5, seed=10, config_json=json.dumps({"content": "AllVanilla"}))
    assert len(turns) == 5 and all(t > 3 for t in turns)


def test_copy_is_independent():
    g = taverngym.Game(3)
    for p in range(8):
        g.step(p, json.loads(g.view(p))["actions"]["CHOOSE"])  # hero option 0
    h = g.copy()
    g.bot_play(0)
    assert h.state_json() != g.state_json()


def test_bad_seat_and_config_raise_value_error():
    import pytest

    g = taverngym.Game(1)
    for call in (g.needs_action, g.legal_mask, g.observe, g.view, g.bot_play):
        with pytest.raises(ValueError):
            call(9)
    with pytest.raises(ValueError):
        g.step(9, 0)
    with pytest.raises(ValueError):
        taverngym.SoloEnv(1, None, 9)
    with pytest.raises(ValueError):
        taverngym.Game(1, json.dumps({"board_size": 9}))


def test_action_cap_forces_end_turn():
    g = taverngym.Game(3, json.dumps({"turn_action_caps": [4]}))
    choose = 30  # first "Choose" action
    for p in range(g.num_players):
        g.step(p, choose)  # hero selection
    # Resolve any choice a hero opened (Discover...), then the turn is a plain recruit phase.
    for _ in range(20):
        mask = np.frombuffer(g.legal_mask(0), dtype=np.uint8)
        if not mask[choose : choose + 16].any():
            break
        g.step(0, choose + int(np.argmax(mask[choose : choose + 16])))
    mask = np.frombuffer(g.legal_mask(0), dtype=np.uint8)
    assert mask[2]  # Freeze is legal
    for _ in range(4 - 0):
        g.step(0, 2)
    mask = np.frombuffer(g.legal_mask(0), dtype=np.uint8)
    assert mask.sum() == 1 and mask[0] == 1  # only End Turn


def test_random_policy_never_meets_an_illegal_action():
    # Every seat, several seeds: the mask is the only thing a policy needs to stay legal,
    # and the observation always has its fixed size.
    rng = np.random.default_rng(1)
    for seed in range(6):
        env = taverngym.BattlegroundsEnv(seed=seed, agent=seed % 8)
        obs, _ = env.reset(seed=100 + seed)
        done, steps = False, 0
        while not done and steps < 20000:
            legal = np.flatnonzero(obs["action_mask"])
            assert legal.size > 0
            a = 0 if (legal[0] == 0 and rng.random() < 0.1) else int(rng.choice(legal))
            obs, reward, done, _, info = env.step(a)
            assert info["illegal"] is None
            assert obs["features"].shape == (taverngym.OBS_FLOATS,)
            assert np.isfinite(obs["features"]).all()
            assert obs["cards"].min() >= 0
            steps += 1
        assert done and 1 <= info["placement"] <= 8


def test_the_python_boundary_refuses_what_it_cannot_play():
    import pytest

    # A misspelt configuration key is an error, not silently dropped.
    with pytest.raises(ValueError, match="hero_choice"):
        taverngym.Game(1, json.dumps({"hero_choice": 4}))
    with pytest.raises(ValueError):
        taverngym.Game(1, json.dumps({"max_combat_attacks": 0}))
    # A saved state from another card table, or with an impossible configuration, is refused at
    # load.
    g = taverngym.Game(3)
    state = json.loads(g.state_json())
    assert taverngym.Game.from_json(json.dumps(state)).turn == g.turn
    other = dict(state, card_table=state["card_table"] ^ 1)
    with pytest.raises(ValueError, match="card table"):
        taverngym.Game.from_json(json.dumps(other))
    short = dict(state, card_table=0)
    short["pool"] = {"counts": state["pool"]["counts"][:3000]}
    with pytest.raises(ValueError, match="pool"):
        taverngym.Game.from_json(json.dumps(short))
    bad = json.loads(g.state_json())
    bad["cfg"]["board_size"] = 12
    with pytest.raises(ValueError):
        taverngym.Game.from_json(json.dumps(bad))


def test_asking_the_bot_does_not_move_the_game():
    g = taverngym.Game(4)
    before = g.state_json()
    answers = {g.bot_action(0) for _ in range(6)}
    assert len(answers) == 1, "the same state gives the same answer"
    assert g.state_json() == before
