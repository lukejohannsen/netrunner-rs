#!/usr/bin/env python3
import argparse
import glob
import json
import os
import random
import torch
import torch.nn as nn
import torch.nn.functional as F
from torch.utils.data import Dataset, DataLoader


class AlphaNetrunnerNet(nn.Module):
    def __init__(self, obs_dim=30, action_dim=724, hidden_dim=256):
        super().__init__()
        self.trunk = nn.Sequential(
            nn.Linear(obs_dim, hidden_dim),
            nn.LayerNorm(hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, hidden_dim),
            nn.LayerNorm(hidden_dim),
            nn.ReLU(),
        )
        self.policy_head = nn.Sequential(
            nn.Linear(hidden_dim, hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, action_dim)
        )
        self.value_head = nn.Sequential(
            nn.Linear(hidden_dim, 64),
            nn.ReLU(),
            nn.Linear(64, 1),
            nn.Tanh()
        )

    def forward(self, obs):
        feat = self.trunk(obs)
        logits = self.policy_head(feat)
        value = self.value_head(feat)
        return logits, value


class NetrunnerTrajectoryDataset(Dataset):
    """Every recorded decision, remembering which game each came from.

    `game_of_sample[i]` is the index of the game sample `i` belongs to. The
    train/validation split is made over games, never over steps: neighbouring
    positions of one game are near-duplicates, so a split over steps puts a
    game on both sides of it and the validation loss falls by memorising
    games rather than judging positions — the loss that read 0.006 while the
    network lost to the uniform search on both sides (ROADMAP Phase 2 §5).
    """

    def __init__(self, data_dir: str):
        self.samples = []
        self.game_of_sample = []
        filepaths = sorted(glob.glob(os.path.join(data_dir, "**", "*.jsonl"), recursive=True))

        game_index = 0
        for filepath in filepaths:
            with open(filepath, "r", encoding="utf-8") as f:
                for line in f:
                    if not line.strip():
                        continue
                    game = json.loads(line)
                    outcome_corp = game["outcome_corp"]
                    game_index += 1

                    for step in game["steps"]:
                        obs = step["observation"]
                        pi = step["policy_target"]
                        active_side = step["active_side"]
                        value_target = outcome_corp if active_side == 0 else -outcome_corp
                        
                        self.samples.append((
                            torch.tensor(obs, dtype=torch.float32),
                            torch.tensor(pi, dtype=torch.float32),
                            torch.tensor(value_target, dtype=torch.float32)
                        ))
                        self.game_of_sample.append(game_index)
        self.game_count = game_index

    def split_by_game(self, val_fraction: float, seed: int = 0):
        """Indices of the training and validation samples, with every game
        wholly on one side. At least one game goes to validation whenever
        there are two or more games."""
        games = list(range(1, self.game_count + 1))
        random.Random(seed).shuffle(games)
        val_count = min(len(games) - 1, max(1, round(len(games) * val_fraction))) if len(games) > 1 else 0
        val_games = set(games[:val_count])
        val_idx = [i for i, g in enumerate(self.game_of_sample) if g in val_games]
        train_idx = [i for i, g in enumerate(self.game_of_sample) if g not in val_games]
        return train_idx, val_idx

    def __len__(self):
        return len(self.samples)

    def __getitem__(self, idx):
        return self.samples[idx]


def export_onnx(model: nn.Module, export_path: str, obs_dim: int):
    model.eval()
    dummy_input = torch.randn(1, obs_dim, dtype=torch.float32)
    torch.onnx.export(
        model,
        dummy_input,
        export_path,
        export_params=True,
        opset_version=17,
        do_constant_folding=True,
        input_names=["obs"],
        output_names=["policy", "value"],
        dynamic_axes={
            "obs": {0: "batch_size"},
            "policy": {0: "batch_size"},
            "value": {0: "batch_size"}
        },
        dynamo=False
    )


def train(args):
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"Using device: {device}")

    dataset = NetrunnerTrajectoryDataset(args.data_dir)
    if len(dataset) == 0:
        raise ValueError(f"No trajectory steps found in '{args.data_dir}'")

    print(f"Successfully loaded {len(dataset)} total decision steps.")

    train_idx, val_idx = dataset.split_by_game(val_fraction=0.1)
    print(f"Split by game: {dataset.game_count} games, {len(train_idx)} training steps, {len(val_idx)} validation steps.")
    train_ds = torch.utils.data.Subset(dataset, train_idx)
    val_ds = torch.utils.data.Subset(dataset, val_idx)

    train_loader = DataLoader(train_ds, batch_size=args.batch_size, shuffle=True)
    val_loader = DataLoader(val_ds, batch_size=args.batch_size, shuffle=False)

    sample_obs, sample_pi, _ = dataset[0]
    obs_dim = sample_obs.shape[0]
    action_dim = sample_pi.shape[0]

    model = AlphaNetrunnerNet(obs_dim=obs_dim, action_dim=action_dim).to(device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)

    best_val_loss = float("inf")
    os.makedirs(args.output_dir, exist_ok=True)
    best_pt_path = os.path.join(args.output_dir, "best_model.pt")

    for epoch in range(1, args.epochs + 1):
        model.train()
        total_p_loss, total_v_loss, total_loss = 0.0, 0.0, 0.0

        for obs, target_pi, target_val in train_loader:
            obs = obs.to(device)
            target_pi = target_pi.to(device)
            target_val = target_val.to(device).unsqueeze(1)

            optimizer.zero_grad()
            logits, val_pred = model(obs)

            log_probs = F.log_softmax(logits, dim=1)
            policy_loss = -torch.sum(target_pi * log_probs, dim=1).mean()
            value_loss = F.mse_loss(val_pred, target_val)

            loss = policy_loss + value_loss
            loss.backward()
            optimizer.step()

            total_p_loss += policy_loss.item() * len(obs)
            total_v_loss += value_loss.item() * len(obs)
            total_loss += loss.item() * len(obs)

        train_p_loss = total_p_loss / len(train_ds)
        train_v_loss = total_v_loss / len(train_ds)
        train_loss = total_loss / len(train_ds)

        val_loss = 0.0
        model.eval()
        with torch.no_grad():
            for obs, target_pi, target_val in val_loader:
                obs = obs.to(device)
                target_pi = target_pi.to(device)
                target_val = target_val.to(device).unsqueeze(1)
                
                logits, val_pred = model(obs)
                log_probs = F.log_softmax(logits, dim=1)
                p_loss = -torch.sum(target_pi * log_probs, dim=1).mean()
                v_loss = F.mse_loss(val_pred, target_val)
                val_loss += (p_loss + v_loss).item() * len(obs)

        val_loss /= len(val_ds)

        print(f"Epoch {epoch:02d}/{args.epochs:02d} | "
              f"Train Loss: {train_loss:.4f} (Policy: {train_p_loss:.4f}, Value: {train_v_loss:.4f}) | "
              f"Val Loss: {val_loss:.4f}")

        # Save PyTorch checkpoint on improvement without running ONNX export mid-loop
        if val_loss < best_val_loss:
            best_val_loss = val_loss
            torch.save(model.state_dict(), best_pt_path)

    # Export ONNX model once training completes
    print("Training complete. Exporting best checkpoint to ONNX...")
    onnx_path = os.path.join(args.output_dir, "netrunner_policy.onnx")
    
    export_model = AlphaNetrunnerNet(obs_dim=obs_dim, action_dim=action_dim).to("cpu")
    export_model.load_state_dict(torch.load(best_pt_path, map_location="cpu"))
    export_onnx(export_model, onnx_path, obs_dim)
    print(f"Model successfully exported to ONNX format at '{onnx_path}'")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-dir", "-d", type=str, required=True)
    parser.add_argument("--output-dir", "-o", type=str, default="./checkpoints")
    parser.add_argument("--epochs", "-e", type=int, default=10)
    parser.add_argument("--batch-size", "-b", type=int, default=64)
    parser.add_argument("--lr", type=float, default=1e-3)
    args = parser.parse_args()
    train(args)
