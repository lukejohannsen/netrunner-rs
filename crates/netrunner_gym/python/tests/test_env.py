"""End-to-end tests against the real `gymnasium.Env` wrapper. Requires the
extension to be built and importable first (`maturin develop` from the
`netrunner_gym` crate directory), plus `numpy`/`gymnasium`/`pytest`
installed — see the crate's `Cargo.toml`/`pyproject.toml` doc comments for
why the underlying Rust logic is *also* covered by `cargo test`, independent
of this file being runnable in any given environment.
"""

import numpy as np
import pytest

from netrunner_gym import ACTION_SPACE_SIZE, NetrunnerGymEnv


def make_env(**kwargs):
    params = {"side": "runner", "seed": 1, "opponent": "random", "max_episode_steps": 50}
    params.update(kwargs)
    return NetrunnerGymEnv(**params)


def test_action_space_size_is_724():
    env = make_env()
    assert ACTION_SPACE_SIZE == 724
    assert env.action_space.n == 724


def test_observation_space_is_a_dict_with_obs_and_action_mask():
    env = make_env()
    obs, info = env.reset(seed=1)

    assert set(env.observation_space.spaces.keys()) == {"obs", "action_mask"}
    assert obs["action_mask"].shape == (724,)
    assert obs["action_mask"].dtype == np.bool_
    assert obs["obs"].shape == (env.observation_space["obs"].shape[0],)
    assert obs["obs"].dtype == np.float32


def test_reset_and_step_both_populate_action_mask_in_obs_and_info():
    env = make_env()
    obs, info = env.reset(seed=2)
    assert "action_mask" in obs
    assert "action_mask" in info
    assert obs["action_mask"].shape == (724,)
    assert info["action_mask"].shape == (724,)

    legal_index = int(np.flatnonzero(obs["action_mask"])[0])
    obs, reward, terminated, truncated, info = env.step(legal_index)
    assert "action_mask" in obs
    assert "action_mask" in info
    assert obs["action_mask"].shape == (724,)


def test_stepping_a_legal_index_advances_the_game_without_raising():
    env = make_env()
    obs, _info = env.reset(seed=3)
    legal_index = int(np.flatnonzero(obs["action_mask"])[0])

    obs2, reward, terminated, truncated, info = env.step(legal_index)
    assert isinstance(reward, float)
    assert isinstance(terminated, bool)
    assert isinstance(truncated, bool)
    assert not info.get("invalid_action", False)


def test_stepping_a_masked_out_index_is_handled_without_crashing_the_episode():
    env = make_env()
    obs, _info = env.reset(seed=4)
    illegal_candidates = np.flatnonzero(~obs["action_mask"])
    assert illegal_candidates.size > 0, "mulligan phase should never have every slot legal"
    illegal_index = int(illegal_candidates[0])

    obs2, reward, terminated, truncated, info = env.step(illegal_index)
    assert info.get("invalid_action") is True
    assert reward < 0.0
    assert terminated is False


def test_stepping_an_out_of_range_index_raises():
    env = make_env()
    env.reset(seed=5)
    with pytest.raises(ValueError):
        env.step(ACTION_SPACE_SIZE + 1)


def test_action_masks_method_matches_observation_mask():
    env = make_env()
    obs, _info = env.reset(seed=6)
    np.testing.assert_array_equal(env.action_masks(), obs["action_mask"])


def test_full_random_episode_reaches_terminated_or_truncated():
    env = make_env(max_episode_steps=100)
    obs, _info = env.reset(seed=7)
    rng = np.random.default_rng(7)

    for _ in range(500):
        legal = np.flatnonzero(obs["action_mask"])
        if legal.size == 0:
            break
        action = int(rng.choice(legal))
        obs, _reward, terminated, truncated, _info = env.step(action)
        if terminated or truncated:
            break
    else:
        pytest.fail("episode did not terminate or truncate within the step budget")
