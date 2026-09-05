"""`ActionSpace`'s segment layout, mirrored from `rules/action_mask.rs`.

The Rust consts are private, so this table is a copy — and a copy of a
layout is exactly the kind of thing that goes stale silently. `SEGMENTS`
is therefore checked against the pinned `ActionSpace::SIZE` at import: if
the lengths stop summing to 1,646 the layout moved and every consumer
fails loudly at startup rather than mislabelling a slot.

Shared by `diagnose_policy_head.py` and `train_alpha_netrunner.py` so the
two cannot disagree about which slots are which.
"""

SIZE = 1646

_H, _I, _REMOTE, _ABIL, _SUBS = 16, 32, 10, 4, 8
_DECK, _ACCESS, _COST, _PENDING, _TRACE = 50, 32, 2, 4, 30
_ZONE = 3 + _REMOTE

_SEGMENT_LENGTHS = [
    ("basic action", 9), ("draw", 2), ("gain credit", 2), ("pass priority", 2),
    ("install card", _H * _ZONE * 2), ("rez ice", _I), ("initiate run", _ZONE),
    ("play event", _H), ("install hardware", _H), ("install program", _H),
    ("play operation", _H), ("discard", _H),
    ("activate ability (corp)", _I * _ABIL), ("activate ability (runner)", _I * _ABIL),
    ("ADVANCE CARD", _I), ("SCORE AGENDA", _I), ("trash resource", _I),
    ("select card to access", _ACCESS), ("steal agenda", 1), ("trash accessed", 1),
    ("pass accessed", 1), ("pay access trigger", 1), ("decline trigger", 1),
    ("corp trace bid", _TRACE + 1), ("runner trace bid", _TRACE + 1),
    ("accept paid choice", 1 + _COST), ("resolve pending choice", _PENDING),
    ("toggle card selection", _DECK), ("confirm card selection", 1),
    ("choose server", _ZONE), ("install resource", _H),
    ("install program on ice", _H * _I), ("break subroutine (click)", _SUBS),
    ("choose trigger", _I),
]

SEGMENTS = []
_off = 0
for _name, _len in _SEGMENT_LENGTHS:
    SEGMENTS.append((_name, _off, _off + _len))
    _off += _len
assert _off == SIZE, f"segment table sums to {_off}, not ActionSpace::SIZE ({SIZE}) -- the layout moved"


def segment_of_slot():
    """`[SIZE]` array mapping each slot to its index in `SEGMENTS`."""
    import numpy as np
    out = np.empty(SIZE, dtype=np.int64)
    for index, (_name, lo, hi) in enumerate(SEGMENTS):
        out[lo:hi] = index
    return out
