"""Gymnasium environment over netrunner_core's fixed 724-slot ActionSpace.

The heavy lifting (game rules, opponent bot, observation encoding) lives in
the compiled Rust extension (``netrunner_gym._netrunner_gym``, built from
the ``netrunner_gym`` Rust crate). This package is a thin, pure-Python
adapter turning that extension's plain lists/tuples into a real
``gymnasium.Env`` with properly-typed ``numpy`` observation/action spaces.
"""

from ._netrunner_gym import ACTION_SPACE_SIZE, OBS_SIZE
from .env import NetrunnerGymEnv

__all__ = ["ACTION_SPACE_SIZE", "OBS_SIZE", "NetrunnerGymEnv"]
