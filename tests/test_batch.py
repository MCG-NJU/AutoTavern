"""The batched environment (`taverngym.BatchEnv`)."""

import json

import numpy as np
import pytest

import taverngym
from taverngym import N_ACTIONS, OBS_CARDS, OBS_FLOATS, BatchEnv


def random_actions(obs, rng):
    """A random legal action per ready slot, ending the turn now and then."""
    out = np.zeros(len(obs["ready"]), dtype=np.int64)
    for s in np.flatnonzero(obs["ready"]):
        legal = np.flatnonzero(obs["action_mask"][s])
        out[s] = 0 if legal[0] == 0 and rng.random() < 0.3 else rng.choice(legal)
    return out


def same(h1, h2):
    return len(h1) == len(h2) and all(
        np.array_equal(u, v) for x, y in zip(h1, h2) for u, v in zip(x, y))


def run(env, steps, seed=0):
    rng = np.random.default_rng(seed)
    history = []
    for _ in range(steps):
        obs = env.observe()
        rewards, dones, info = env.step(random_actions(obs, rng))
        history.append((rewards.copy(), dones.copy(), info["placement"].copy()))
    return history


def test_shapes_and_slots():
    env = BatchEnv(3, seed=1, controlled=(0, 5), threads=2)
    assert (env.n_games, env.n_slots, env.controlled) == (3, 6, (0, 5))
    assert [env.slot(i) for i in range(6)] == [(0, 0), (0, 5), (1, 0), (1, 5), (2, 0), (2, 5)]
    obs = env.observe()
    assert obs["features"].shape == (6, OBS_FLOATS) and obs["features"].dtype == np.float32
    assert obs["cards"].shape == (6, OBS_CARDS) and obs["cards"].dtype == np.int32
    assert obs["action_mask"].shape == (6, N_ACTIONS) and obs["ready"].shape == (6,)
    # A ready slot has at least one legal action; a waiting slot none.
    for s in range(6):
        assert obs["action_mask"][s].any() == obs["ready"][s]


def test_episodes_end_and_games_restart():
    env = BatchEnv(2, seed=3, controlled=(0, 1, 2), threads=2)
    history = run(env, 1500)
    places = [p for _, dones, placement in history for p, d in zip(placement, dones) if d]
    assert places and all(1 <= p <= 8 for p in places)
    rewards = np.concatenate([r for r, _, _ in history])
    assert rewards.min() >= -1.0 and rewards.max() <= 1.0
    assert max(env.episode_index(i) for i in range(2)) >= 1, "a finished game restarts"


def test_threads_do_not_change_results():
    a = run(BatchEnv(4, seed=7, controlled=(0,), threads=1), 300)
    b = run(BatchEnv(4, seed=7, controlled=(0,), threads=4), 300)
    assert same(a, b)


def test_rejected_actions_are_ignored():
    env = BatchEnv(1, seed=2)
    before = env.save(0)
    rewards, dones, info = env.step([N_ACTIONS + 5])
    assert info["rejected"][0] and rewards[0] < 0 and not dones[0]
    assert env.save(0) == before
    with pytest.raises(ValueError):
        env.step([0, 0])


def test_save_and_load_continue_exactly():
    a = BatchEnv(1, seed=5)
    run(a, 40, seed=1)
    b = BatchEnv(1, seed=5)  # fresh; the same seed for the games after this one
    b.load(0, a.save(0))
    assert same(run(a, 60, seed=2), run(b, 60, seed=2))
    assert a.save(0) == b.save(0)
    bad = json.loads(a.save(0))
    bad["game"]["card_table"] ^= 1
    with pytest.raises(ValueError, match="card table"):
        b.load(0, json.dumps(bad))


def test_a_game_copy_can_be_inspected():
    env = BatchEnv(2, seed=4)
    g = env.game(1)
    assert "me" in json.loads(g.view(0))
    assert isinstance(g, taverngym.Game)
    assert json.loads(g.state_json())["turn"] == json.loads(env.save(1))["game"]["turn"]


def test_bad_arguments():
    for kwargs in (dict(n_games=0), dict(n_games=1, controlled=()),
                   dict(n_games=1, controlled=(8,)), dict(n_games=1, controlled=(1, 1))):
        with pytest.raises(ValueError):
            BatchEnv(**kwargs)
    with pytest.raises(ValueError, match="hero_choice"):
        BatchEnv(1, config={"hero_choice": 4})
