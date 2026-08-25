"""The actual `gymnasium.Env` subclass, layered over the compiled Rust
extension (`netrunner_gym._netrunner_gym`).

Design note: the Rust/PyO3 boundary (`_netrunner_gym.NetrunnerEnv`) only
ever crosses plain Python `list`/`tuple`/`bool`/`float`/`int`/`str` values
— no `numpy` dependency in the compiled extension itself. This module is
the only layer that needs `numpy`/`gymnasium` installed; it converts those
plain lists into the properly-dtyped `np.ndarray`s Gymnasium/Stable-
Baselines3 expect.
"""

from __future__ import annotations

from typing import Any, Optional

import gymnasium as gym
import numpy as np
from gymnasium import spaces

from . import _netrunner_gym as native


class NetrunnerGymEnv(gym.Env):
    """A single-agent Netrunner environment: `side` plays through
    `netrunner_core`'s rules engine via the fixed `ActionSpace.SIZE == 724`
    categorical action index; the other side is driven internally by a
    baseline `netrunner_bots` agent (`opponent`) between the caller's own
    `step()` calls — see the Rust crate's `env` module doc comment for how
    every kind of opponent decision (mulligan, discard, paid-ability
    windows, trace bids, run choices) is fast-forwarded uniformly.
    """

    metadata: dict[str, Any] = {"render_modes": []}

    def __init__(
        self,
        side: str = "runner",
        seed: int = 0,
        opponent: str = "heuristic",
        max_episode_steps: int = 200,
    ) -> None:
        super().__init__()
        self._env = native.NetrunnerEnv(side, seed, opponent, max_episode_steps)

        self.action_space = spaces.Discrete(native.ACTION_SPACE_SIZE)
        self.observation_space = spaces.Dict(
            {
                # `netrunner_gym::observation::encode_observation` (Rust)
                # normalizes every feature into `[-1.0, 1.0]` — see that
                # module's own `all_features_are_finite_and_within_a_sane_range`
                # test — so a bounded `Box` here is accurate, not just a
                # loose default.
                "obs": spaces.Box(low=-1.0, high=1.0, shape=(native.OBS_SIZE,), dtype=np.float32),
                "action_mask": spaces.Box(low=0, high=1, shape=(native.ACTION_SPACE_SIZE,), dtype=np.bool_),
            }
        )

    def reset(
        self,
        *,
        seed: Optional[int] = None,
        options: Optional[dict[str, Any]] = None,
    ) -> tuple[dict[str, np.ndarray], dict[str, Any]]:
        super().reset(seed=seed)
        obs, mask = self._env.reset(seed)
        observation = self._pack(obs, mask)
        info = {"action_mask": observation["action_mask"]}
        return observation, info

    def step(self, action_index: int) -> tuple[dict[str, np.ndarray], float, bool, bool, dict[str, Any]]:
        obs, mask, reward, terminated, truncated, invalid_action, message = self._env.step(int(action_index))
        observation = self._pack(obs, mask)
        info: dict[str, Any] = {"action_mask": observation["action_mask"]}
        if invalid_action:
            info["invalid_action"] = True
            info["error"] = message
        return observation, float(reward), bool(terminated), bool(truncated), info

    def action_masks(self) -> np.ndarray:
        """`sb3-contrib`'s `MaskablePPO`/`ActionMasker` convention: a
        method returning the current action mask, independent of the
        `info`/observation dict plumbing above."""
        return np.asarray(self._env.action_mask(), dtype=np.bool_)

    def render(self) -> None:  # pragma: no cover - no render modes declared
        return None

    @staticmethod
    def _pack(obs: list[float], mask: list[bool]) -> dict[str, np.ndarray]:
        return {
            "obs": np.asarray(obs, dtype=np.float32),
            "action_mask": np.asarray(mask, dtype=np.bool_),
        }
