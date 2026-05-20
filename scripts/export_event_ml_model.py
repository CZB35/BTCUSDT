#!/usr/bin/env python3
"""Train and export the BTCUSDT event-contract ExtraTrees model for Rust.

The exported JSON is intentionally simple: each tree stores child indexes,
feature indexes, thresholds, and the positive-class probability at every node.
Rust can then reproduce sklearn ExtraTrees predict_proba by averaging trees.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
from types import SimpleNamespace

import numpy as np

import train_event_1s_two_stage as research


MODEL_VERSION = "event-edge-ml-v1"


def tree_to_json(estimator, positive_index: int) -> dict:
    tree = estimator.tree_
    values = tree.value[:, 0, :]
    totals = values.sum(axis=1)
    positive_prob = np.divide(
        values[:, positive_index],
        totals,
        out=np.zeros(values.shape[0], dtype=np.float64),
        where=totals > 0,
    )
    return {
        "children_left": tree.children_left.astype(int).tolist(),
        "children_right": tree.children_right.astype(int).tolist(),
        "feature": tree.feature.astype(int).tolist(),
        "threshold": tree.threshold.astype(float).tolist(),
        "positive_prob": positive_prob.astype(float).tolist(),
    }


def forest_to_json(model) -> dict:
    classes = [int(value) for value in model.classes_.tolist()]
    positive_index = classes.index(1)
    return {
        "classes": classes,
        "positive_class": 1,
        "trees": [tree_to_json(estimator, positive_index) for estimator in model.estimators_],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--db", default="event_signal_train")
    parser.add_argument("--db-user", default="postgres")
    parser.add_argument("--psql", default=r"D:\pgsql\bin\psql.exe")
    parser.add_argument("--load-days", type=int, default=100)
    parser.add_argument("--train-days", type=int, default=90)
    parser.add_argument("--sample-stride", type=int, default=120)
    parser.add_argument("--tree-estimators", type=int, default=220)
    parser.add_argument("--extra-max-depth", type=int, default=16)
    parser.add_argument("--extra-min-samples-leaf", type=int, default=100)
    parser.add_argument("--extra-max-features", default="sqrt")
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--move-label-bp", type=float, default=4.0)
    parser.add_argument("--move-threshold", type=float, default=0.68)
    parser.add_argument("--direction-threshold", type=float, default=0.58)
    parser.add_argument("--output", default="models/event_edge_ml_v1.json")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    model_args = SimpleNamespace(
        symbol=args.symbol,
        db=args.db,
        db_user=args.db_user,
        psql=args.psql,
        load_days=args.load_days,
        model="extra",
        tree_estimators=args.tree_estimators,
        extra_max_depth=args.extra_max_depth,
        extra_min_samples_leaf=args.extra_min_samples_leaf,
        extra_max_features=args.extra_max_features,
        jobs=args.jobs,
    )
    df = research.load_prices(model_args)
    df, features = research.add_features(df)
    research.apply_label_delay(df, 0)
    feature_mask = df[features].notna().all(axis=1) & df["future_ret"].notna()
    end_ms = int(df["second_ms"].iloc[-1])
    train_start_ms = end_ms - args.train_days * 86_400_000
    train_mask = (df["second_ms"] >= train_start_ms) & feature_mask
    train_idx = np.flatnonzero(train_mask.to_numpy())
    train_idx = train_idx[train_idx % args.sample_stride == 0]
    move_label = (df["future_ret"].abs() >= args.move_label_bp / 10000.0).astype("int8")
    dir_train_idx = train_idx[move_label.iloc[train_idx].to_numpy() == 1]
    if len(train_idx) < 20_000 or len(dir_train_idx) < 5_000:
        raise RuntimeError(
            f"not enough training rows: move={len(train_idx)} direction={len(dir_train_idx)}"
        )

    x_train = df.iloc[train_idx][features].astype("float32")
    x_dir_train = df.iloc[dir_train_idx][features].astype("float32")
    move_model = research.make_model(model_args)
    direction_model = research.make_model(model_args)
    move_model.fit(x_train, move_label.iloc[train_idx].astype("int8"))
    direction_model.fit(x_dir_train, df.iloc[dir_train_idx]["label_up"].astype("int8"))

    payload = {
        "version": MODEL_VERSION,
        "symbol": args.symbol,
        "period_seconds": research.SETTLE_SECONDS,
        "created_by": "scripts/export_event_ml_model.py",
        "training": {
            "load_days": args.load_days,
            "train_days": args.train_days,
            "sample_stride": args.sample_stride,
            "move_label_bp": args.move_label_bp,
            "rows": int(len(df)),
            "train_rows": int(len(train_idx)),
            "direction_train_rows": int(len(dir_train_idx)),
            "start_time": str(df["ts_utc"].iloc[0]),
            "end_time": str(df["ts_utc"].iloc[-1]),
        },
        "thresholds": {
            "move_probability": args.move_threshold,
            "direction_probability": args.direction_threshold,
        },
        "execution_filter": {
            "up_falling_knife_min_body_share_60": 0.66,
            "up_falling_knife_max_lower_wick_share_60": 0.30,
        },
        "feature_names": features,
        "models": {
            "move": forest_to_json(move_model),
            "direction": forest_to_json(direction_model),
        },
    }
    output = pathlib.Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", encoding="utf-8") as file:
        json.dump(payload, file, separators=(",", ":"))
    print(
        f"exported {MODEL_VERSION} features={len(features)} "
        f"move_trees={len(move_model.estimators_)} direction_trees={len(direction_model.estimators_)} "
        f"path={output}",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
