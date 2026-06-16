#!/usr/bin/env python3
"""Train and export the BTCUSDT event-contract ExtraTrees model for Rust.

The exported JSON is intentionally simple: each tree stores child indexes,
feature indexes, thresholds, and the positive-class probability at every node.
Rust can then reproduce sklearn ExtraTrees predict_proba by averaging trees.
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import json
import pathlib
import sys
import time
import urllib.request
import zipfile
from types import SimpleNamespace

import numpy as np
import pandas as pd

import train_event_1s_two_stage as research


DATA_BASE_URL = "https://data.binance.vision/data/futures/um/daily/aggTrades"
SECONDS_PER_DAY = 24 * 60 * 60
MODEL_VERSION = "event-edge-ml-v2"


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
    parser.add_argument("--cache-dir", default="")
    parser.add_argument("--start-date", default="")
    parser.add_argument("--days", type=int, default=0)
    parser.add_argument("--download-missing", action="store_true")
    parser.add_argument("--quiet-days", action="store_true")
    parser.add_argument("--retries", type=int, default=4)
    parser.add_argument("--load-days", type=int, default=100)
    parser.add_argument("--train-days", type=int, default=90)
    parser.add_argument("--sample-stride", type=int, default=120)
    parser.add_argument("--tree-estimators", type=int, default=220)
    parser.add_argument("--extra-max-depth", type=int, default=16)
    parser.add_argument("--extra-min-samples-leaf", type=int, default=100)
    parser.add_argument("--extra-max-features", default="sqrt")
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--move-label-bp", type=float, default=4.0)
    parser.add_argument("--settle-seconds", type=int, default=research.SETTLE_SECONDS)
    parser.add_argument("--move-threshold", type=float, default=0.68)
    parser.add_argument("--direction-threshold", type=float, default=0.58)
    parser.add_argument("--model-version", default=MODEL_VERSION)
    parser.add_argument("--feature-template", default="models/event_edge_ml_v1.json")
    parser.add_argument("--output", default="models/event_edge_ml_v1.json")
    return parser.parse_args()


def utc_ms(day: dt.date) -> int:
    instant = dt.datetime(day.year, day.month, day.day, tzinfo=dt.timezone.utc)
    return int(instant.timestamp() * 1000)


def download_zip(args: argparse.Namespace, day: dt.date) -> pathlib.Path:
    cache_dir = pathlib.Path(args.cache_dir or ".codex/binance-cache")
    cache_dir.mkdir(parents=True, exist_ok=True)
    filename = f"{args.symbol}-aggTrades-{day.isoformat()}.zip"
    target = cache_dir / filename
    if target.exists() and target.stat().st_size > 0:
        return target
    url = f"{DATA_BASE_URL}/{args.symbol}/{filename}"
    temp_target = target.with_suffix(".zip.part")
    for attempt in range(1, args.retries + 1):
        try:
            print(f"download {url} attempt {attempt}/{args.retries}", flush=True)
            temp_target.unlink(missing_ok=True)
            urllib.request.urlretrieve(url, temp_target)
            temp_target.replace(target)
            return target
        except Exception as exc:
            temp_target.unlink(missing_ok=True)
            if attempt >= args.retries:
                raise
            sleep_seconds = min(30, attempt * 3)
            print(f"download failed: {exc}; retry in {sleep_seconds}s", flush=True)
            time.sleep(sleep_seconds)
    return target


def ensure_zip(args: argparse.Namespace, day: dt.date) -> pathlib.Path:
    cache_dir = pathlib.Path(args.cache_dir or ".codex/binance-cache")
    filename = f"{args.symbol}-aggTrades-{day.isoformat()}.zip"
    zip_path = cache_dir / filename
    if zip_path.exists() and zip_path.stat().st_size > 0:
        return zip_path
    if not args.download_missing:
        raise FileNotFoundError(f"missing cached file: {zip_path}")
    return download_zip(args, day)


def aggregate_zip(zip_path: pathlib.Path) -> tuple[dict[int, dict[str, float | int]], int]:
    buckets: dict[int, dict[str, float | int]] = {}
    row_count = 0
    with zipfile.ZipFile(zip_path) as archive:
        names = archive.namelist()
        if not names:
            raise RuntimeError(f"empty zip: {zip_path}")
        with archive.open(names[0]) as raw:
            reader = csv.DictReader(line.decode("utf-8") for line in raw)
            for row in reader:
                row_count += 1
                second_ms = int(row["transact_time"]) // 1000 * 1000
                agg_id = int(row["agg_trade_id"])
                first_trade_id = int(row["first_trade_id"])
                last_trade_id = int(row["last_trade_id"])
                price = float(row["price"])
                quantity = float(row["quantity"])
                is_buyer_maker = row["is_buyer_maker"].strip().lower() == "true"
                trade_count = max(1, last_trade_id - first_trade_id + 1)
                bucket = buckets.get(second_ms)
                if bucket is None:
                    bucket = {
                        "open": price,
                        "high": price,
                        "low": price,
                        "close": price,
                        "volume": 0.0,
                        "taker_buy_volume": 0.0,
                        "taker_sell_volume": 0.0,
                        "agg_count": 0,
                        "trade_count": 0,
                        "first_agg_trade_id": agg_id,
                        "last_agg_trade_id": agg_id,
                    }
                    buckets[second_ms] = bucket
                bucket["high"] = max(float(bucket["high"]), price)
                bucket["low"] = min(float(bucket["low"]), price)
                bucket["close"] = price
                bucket["volume"] = float(bucket["volume"]) + quantity
                if is_buyer_maker:
                    bucket["taker_sell_volume"] = float(bucket["taker_sell_volume"]) + quantity
                else:
                    bucket["taker_buy_volume"] = float(bucket["taker_buy_volume"]) + quantity
                bucket["agg_count"] = int(bucket["agg_count"]) + 1
                bucket["trade_count"] = int(bucket["trade_count"]) + trade_count
                bucket["first_agg_trade_id"] = min(int(bucket["first_agg_trade_id"]), agg_id)
                bucket["last_agg_trade_id"] = max(int(bucket["last_agg_trade_id"]), agg_id)
    return buckets, row_count


def append_filled_day(
    rows: dict[str, list],
    *,
    symbol: str,
    day: dt.date,
    buckets: dict[int, dict[str, float | int]],
    last_close: float | None,
    quiet: bool = False,
) -> float:
    start_ms = utc_ms(day)
    first_bucket = buckets[min(buckets)] if buckets else None
    if last_close is None:
        if first_bucket is None:
            raise RuntimeError(f"no trades available to seed first day: {day}")
        last_close = float(first_bucket["open"])

    traded_seconds = 0
    for second in range(SECONDS_PER_DAY):
        second_ms = start_ms + second * 1000
        bucket = buckets.get(second_ms)
        if bucket is None:
            open_price = high = low = close = last_close
            volume = buy_volume = sell_volume = 0.0
            agg_count = trade_count = 0
            has_trade = False
        else:
            open_price = float(bucket["open"])
            high = float(bucket["high"])
            low = float(bucket["low"])
            close = float(bucket["close"])
            volume = float(bucket["volume"])
            buy_volume = float(bucket["taker_buy_volume"])
            sell_volume = float(bucket["taker_sell_volume"])
            agg_count = int(bucket["agg_count"])
            trade_count = int(bucket["trade_count"])
            has_trade = True
            traded_seconds += 1

        rows["symbol"].append(symbol)
        rows["second_ms"].append(second_ms)
        rows["open"].append(open_price)
        rows["high"].append(high)
        rows["low"].append(low)
        rows["close"].append(close)
        rows["volume"].append(volume)
        rows["taker_buy_volume"].append(buy_volume)
        rows["taker_sell_volume"].append(sell_volume)
        rows["agg_count"].append(agg_count)
        rows["trade_count"].append(trade_count)
        rows["has_trade"].append(has_trade)
        last_close = close

    if not quiet:
        print(
            f"loaded {day.isoformat()} traded_seconds={traded_seconds} "
            f"filled_seconds={SECONDS_PER_DAY - traded_seconds}",
            flush=True,
        )
    return last_close


def load_cache_prices(args: argparse.Namespace) -> pd.DataFrame:
    rows: dict[str, list] = {
        "symbol": [],
        "second_ms": [],
        "open": [],
        "high": [],
        "low": [],
        "close": [],
        "volume": [],
        "taker_buy_volume": [],
        "taker_sell_volume": [],
        "agg_count": [],
        "trade_count": [],
        "has_trade": [],
    }
    start = dt.date.fromisoformat(args.start_date)
    last_close: float | None = None
    for offset in range(args.days):
        day = start + dt.timedelta(days=offset)
        zip_path = ensure_zip(args, day)
        buckets, source_rows = aggregate_zip(zip_path)
        if not args.quiet_days:
            print(f"aggregate {zip_path.name} source_rows={source_rows}", flush=True)
        last_close = append_filled_day(
            rows,
            symbol=args.symbol,
            day=day,
            buckets=buckets,
            last_close=last_close,
            quiet=args.quiet_days,
        )

    df = pd.DataFrame(rows)
    df["ts_utc"] = pd.to_datetime(df["second_ms"], unit="ms", utc=True)
    for column in [
        "open",
        "high",
        "low",
        "close",
        "volume",
        "taker_buy_volume",
        "taker_sell_volume",
    ]:
        df[column] = df[column].astype("float64")
    df["trade_count"] = df["trade_count"].astype("int64")
    return df


def load_prices(args: argparse.Namespace) -> tuple[object, str]:
    if bool(args.start_date) or args.days > 0 or bool(args.cache_dir):
        if not args.start_date or args.days <= 0:
            raise ValueError("--start-date and --days are required for cache training")

        cache_args = SimpleNamespace(
            symbol=args.symbol,
            cache_dir=args.cache_dir or ".codex/binance-cache",
            start_date=args.start_date,
            days=args.days,
            download_missing=args.download_missing,
            quiet_days=args.quiet_days,
            retries=args.retries,
        )
        return load_cache_prices(cache_args), "cache"

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
    return research.load_prices(model_args), "database"


def apply_feature_template(features: list[str], template_path: str) -> list[str]:
    if not template_path:
        return features
    path = pathlib.Path(template_path)
    if not path.exists():
        return features
    with path.open(encoding="utf-8") as file:
        payload = json.load(file)
    template_features = payload.get("feature_names")
    if not isinstance(template_features, list) or not template_features:
        return features
    missing = [name for name in template_features if name not in features]
    if missing:
        raise RuntimeError(
            f"feature template contains names not produced by add_features: {missing[:8]}"
        )
    return [str(name) for name in template_features]


def main() -> int:
    args = parse_args()
    research.set_settle_seconds(args.settle_seconds)
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
    df, data_source = load_prices(args)
    df, features = research.add_features(df)
    raw_feature_count = len(features)
    features = apply_feature_template(features, args.feature_template)
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
        "version": args.model_version,
        "symbol": args.symbol,
        "period_seconds": research.SETTLE_SECONDS,
        "created_by": "scripts/export_event_ml_model.py",
        "training": {
            "data_source": data_source,
            "load_days": args.load_days,
            "cache_start_date": args.start_date,
            "cache_days": args.days,
            "train_days": args.train_days,
            "sample_stride": args.sample_stride,
            "move_label_bp": args.move_label_bp,
            "rows": int(len(df)),
            "train_rows": int(len(train_idx)),
            "direction_train_rows": int(len(dir_train_idx)),
            "raw_feature_count": raw_feature_count,
            "feature_template": args.feature_template,
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
        f"exported {args.model_version} source={data_source} features={len(features)} "
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
