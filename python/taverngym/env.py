"""Gym-style single-agent environment (one learning seat vs. 7 heuristic bots).

Observation: dict with
    "features": float32[OBS_FLOATS]   numeric features (board, hand, shop, choices, opponents)
    "cards":    int32[OBS_CARDS]      card embedding indices (0 = empty; see taverngym.card_info)
    "action_mask": bool[N_ACTIONS]
Reward: 0 until the episode ends, then placement-based in [-1, 1] (1st = 1, 8th = -1).
Illegal actions are ignored and cost `illegal_penalty` (default -0.01).
"""

import json

import numpy as np

from taverngym._core import N_ACTIONS, OBS_CARDS, OBS_FLOATS, SoloEnv


class BattlegroundsEnv:
    def __init__(self, seed=0, config=None, agent=0):
        self._config_json = json.dumps(config) if config is not None else None
        self._seed = seed
        self._agent = agent
        self._env = SoloEnv(seed, self._config_json, agent)
        self.n_actions = N_ACTIONS

    def _obs(self):
        f, c = self._env.observe()
        return {
            "features": np.frombuffer(f, dtype="<f4").copy(),
            "cards": np.frombuffer(c, dtype="<i4").copy(),
            "action_mask": np.frombuffer(self._env.legal_mask(), dtype=np.uint8).astype(bool),
        }

    def reset(self, seed=None):
        if seed is None:
            self._seed += 1
        else:
            self._seed = seed
        self._env.reset(self._seed)
        return self._obs(), {"turn": self._env.turn}

    def step(self, action):
        reward, done, illegal = self._env.step(int(action))
        info = {"turn": self._env.turn, "placement": self._env.placement, "illegal": illegal}
        return self._obs(), reward, done, False, info

    def state_json(self):
        return self._env.state_json()

    @property
    def observation_sizes(self):
        return {"features": OBS_FLOATS, "cards": OBS_CARDS, "action_mask": N_ACTIONS}
