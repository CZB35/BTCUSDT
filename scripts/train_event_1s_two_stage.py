#!/usr/bin/env python3
"""Two-stage research model for seconds-accurate BTCUSDT event contracts.

Offline only. The event result is measured as:
close(entry_second + settle_seconds) > close(entry_second).
"""

from __future__ import annotations

import argparse
import math
import shutil
import subprocess
import warnings
from copy import deepcopy

import numpy as np
import pandas as pd
from pandas.errors import PerformanceWarning
from sklearn.ensemble import ExtraTreesClassifier, HistGradientBoostingClassifier

try:
    from xgboost import XGBClassifier
except ImportError:  # pragma: no cover - optional research dependency
    XGBClassifier = None

try:
    from catboost import CatBoostClassifier
except ImportError:  # pragma: no cover - optional research dependency
    CatBoostClassifier = None


SETTLE_SECONDS = 1800
WIN_USDT = 4.0
LOSS_USDT = -5.0
BREAKEVEN = 5.0 / 9.0


def set_settle_seconds(seconds: int) -> None:
    if seconds <= 0:
        raise ValueError("settle seconds must be positive")
    global SETTLE_SECONDS
    SETTLE_SECONDS = seconds


def load_prices(args: argparse.Namespace) -> pd.DataFrame:
    load_filter = ""
    if args.load_days > 0:
        load_ms = int(args.load_days * 86_400_000)
        load_filter = f"""
          AND second_ms >= (
              SELECT max(second_ms) - {load_ms}
              FROM binance_futures_price_1s
              WHERE symbol = '{args.symbol}'
          )
        """
    query = f"""
        SELECT second_ms,
               ts_utc,
               open::float8 AS open,
               high::float8 AS high,
               low::float8 AS low,
               close::float8 AS close,
               volume::float8 AS volume,
               taker_buy_volume::float8 AS taker_buy_volume,
               taker_sell_volume::float8 AS taker_sell_volume,
               trade_count::int AS trade_count
        FROM binance_futures_price_1s
        WHERE symbol = '{args.symbol}'
          {load_filter}
        ORDER BY second_ms
    """
    copy = f"\\copy ({query}) TO STDOUT WITH (FORMAT csv, HEADER true)"
    cmd = [
        args.psql,
        "-v",
        "ON_ERROR_STOP=1",
        "-U",
        args.db_user,
        "-d",
        args.db,
        "-c",
        copy,
    ]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    assert proc.stdout is not None
    df = pd.read_csv(proc.stdout)
    _, stderr = proc.communicate()
    if proc.returncode != 0:
        raise RuntimeError(stderr.strip())
    df["ts_utc"] = pd.to_datetime(df["ts_utc"])
    return df


def add_features(df: pd.DataFrame) -> tuple[pd.DataFrame, list[str]]:
    close = df["close"].astype("float64")
    high = df["high"].astype("float64")
    low = df["low"].astype("float64")
    volume = df["volume"].astype("float64")
    buy = df["taker_buy_volume"].astype("float64")
    sell = df["taker_sell_volume"].astype("float64")
    trades = df["trade_count"].astype("float64")
    pct = close.pct_change()

    features: list[str] = []
    for sec in [1, 2, 3, 5, 10, 15, 30, 45, 60, 90, 120, 180, 300, 420, 600, 900, 1800, 3600]:
        name = f"ret_{sec}"
        df[name] = close / close.shift(sec) - 1.0
        features.append(name)

    for sec in [15, 30, 60, 120, 300, 420, 600, 900, 1800, 3600]:
        vol_name = f"vol_{sec}"
        df[vol_name] = pct.rolling(sec, min_periods=sec).std()
        features.append(vol_name)

        rolling_low = close.rolling(sec, min_periods=sec).min()
        rolling_high = close.rolling(sec, min_periods=sec).max()
        price_range = (rolling_high - rolling_low).replace(0, np.nan)
        for name, series in [
            (f"pos_{sec}", (close - rolling_low) / price_range),
            (f"range_ret_{sec}", price_range / close.shift(sec)),
            (f"dist_high_{sec}", close / rolling_high - 1.0),
            (f"dist_low_{sec}", close / rolling_low - 1.0),
        ]:
            df[name] = series
            features.append(name)

    # Minute-level candle structure built from 1-second data. The model still
    # settles by exact second, but these features suppress second-level noise.
    for sec in [60, 180, 300, 600]:
        open_price = close.shift(sec)
        rolling_high = high.rolling(sec, min_periods=sec).max()
        rolling_low = low.rolling(sec, min_periods=sec).min()
        candle_high = pd.Series(
            np.maximum(np.maximum(rolling_high.to_numpy(), open_price.to_numpy()), close.to_numpy()),
            index=df.index,
        )
        candle_low = pd.Series(
            np.minimum(np.minimum(rolling_low.to_numpy(), open_price.to_numpy()), close.to_numpy()),
            index=df.index,
        )
        candle_range = (candle_high - candle_low).replace(0, np.nan)
        body_abs = (close - open_price).abs()
        upper_body = pd.Series(np.maximum(open_price.to_numpy(), close.to_numpy()), index=df.index)
        lower_body = pd.Series(np.minimum(open_price.to_numpy(), close.to_numpy()), index=df.index)
        for name, series in [
            (f"candle_body_bp_{sec}", (close / open_price - 1.0) * 10000.0),
            (f"candle_body_share_{sec}", body_abs / candle_range),
            (f"candle_upper_wick_share_{sec}", (candle_high - upper_body) / candle_range),
            (f"candle_lower_wick_share_{sec}", (lower_body - candle_low) / candle_range),
            (f"candle_close_pos_{sec}", (close - candle_low) / candle_range),
            (f"candle_range_bp_{sec}", candle_range / open_price * 10000.0),
        ]:
            df[name] = series
            features.append(name)

    for sec in [15, 30, 60, 120, 300, 600, 900]:
        buy_sum = buy.rolling(sec, min_periods=sec).sum()
        sell_sum = sell.rolling(sec, min_periods=sec).sum()
        vol_sum = volume.rolling(sec, min_periods=sec).sum()
        trade_sum = trades.rolling(sec, min_periods=sec).sum()
        total = (buy_sum + sell_sum).replace(0, np.nan)
        for name, series in [
            (f"buy_ratio_{sec}", buy_sum / total),
            (f"buy_imbalance_{sec}", (buy_sum - sell_sum) / total),
            (f"log_volume_{sec}", np.log1p(vol_sum)),
            (f"log_trades_{sec}", np.log1p(trade_sum)),
        ]:
            df[name] = series
            features.append(name)

    for short, long in [(10, 60), (30, 120), (60, 300), (120, 600), (300, 1800)]:
        name = f"ret_{short}_minus_scaled_{long}"
        df[name] = df[f"ret_{short}"] - df[f"ret_{long}"] * (short / long)
        features.append(name)

    for short, long in [(30, 300), (60, 900), (120, 1800)]:
        name = f"vol_{short}_vs_{long}"
        df[name] = df[f"vol_{short}"] / df[f"vol_{long}"].replace(0, np.nan)
        features.append(name)

    seconds_of_day = (
        df["ts_utc"].dt.hour * 3600 + df["ts_utc"].dt.minute * 60 + df["ts_utc"].dt.second
    ).astype("float64")
    event_pos = (seconds_of_day % SETTLE_SECONDS) / SETTLE_SECONDS
    for name, series in [
        ("event_pos", event_pos),
        ("event_sin", np.sin(2 * math.pi * event_pos)),
        ("event_cos", np.cos(2 * math.pi * event_pos)),
        ("tod_sin", np.sin(2 * math.pi * seconds_of_day / 86400.0)),
        ("tod_cos", np.cos(2 * math.pi * seconds_of_day / 86400.0)),
    ]:
        df[name] = series
        features.append(name)

    df["future_close"] = close.shift(-SETTLE_SECONDS)
    df["future_ret"] = df["future_close"] / close - 1.0
    df["label_up"] = (df["future_ret"] > 0).astype("int8")
    return df, features


def apply_label_delay(df: pd.DataFrame, delay_seconds: int) -> None:
    """Set the supervised label to match the intended execution delay.

    Features are still observed at signal time t. With delay_seconds=5 the
    label becomes close(t + delay + settle_seconds) / close(t + delay) - 1.
    """
    if delay_seconds < 0:
        raise ValueError("label delay must be >= 0")
    close = df["close"].astype("float64")
    entry_close = close.shift(-delay_seconds) if delay_seconds else close
    future_close = close.shift(-(SETTLE_SECONDS + delay_seconds))
    df["future_close"] = future_close
    df["future_ret"] = future_close / entry_close - 1.0
    df["label_up"] = (df["future_ret"] > 0).astype("int8")


def minute_structure_signal(df: pd.DataFrame) -> np.ndarray:
    """Return a simple candidate direction from minute-level structure.

    1 means only consider UP, -1 means only consider DOWN, 0 means no trade.
    The model is still required to agree before a trade is counted.
    """
    signal = np.zeros(len(df), dtype=np.int8)
    ret_60 = df["ret_60"].to_numpy()
    ret_180 = df["ret_180"].to_numpy()
    ret_300 = df["ret_300"].to_numpy()
    ret_600 = df["ret_600"].to_numpy()
    pos_300 = df["pos_300"].to_numpy()
    pos_900 = df["pos_900"].to_numpy()
    vol_60_vs_900 = df["vol_60_vs_900"].to_numpy()
    imb_60 = df["buy_imbalance_60"].to_numpy()
    imb_300 = df["buy_imbalance_300"].to_numpy()
    body_60 = df["candle_body_bp_60"].to_numpy()
    body_share_60 = df["candle_body_share_60"].to_numpy()
    upper_wick_60 = df["candle_upper_wick_share_60"].to_numpy()
    lower_wick_60 = df["candle_lower_wick_share_60"].to_numpy()
    close_pos_60 = df["candle_close_pos_60"].to_numpy()
    range_60 = df["candle_range_bp_60"].to_numpy()

    real_up = (body_60 > 1.2) & (body_share_60 > 0.34) & (upper_wick_60 < 0.42)
    real_down = (body_60 < -1.2) & (body_share_60 > 0.34) & (lower_wick_60 < 0.42)
    fake_up = (upper_wick_60 > 0.48) & (close_pos_60 < 0.58)
    fake_down = (lower_wick_60 > 0.48) & (close_pos_60 > 0.42)
    enough_move = range_60 >= 2.0

    trend_up = (
        (ret_180 > 0.00035)
        & (ret_300 > 0.00055)
        & (ret_60 > -0.00008)
        & (pos_300 > 0.35)
        & (pos_300 < 0.86)
        & real_up
        & enough_move
        & (imb_60 > imb_300 - 0.08)
    )
    trend_down = (
        (ret_180 < -0.00035)
        & (ret_300 < -0.00055)
        & (ret_60 < 0.00008)
        & (pos_300 < 0.65)
        & (pos_300 > 0.14)
        & real_down
        & enough_move
        & (imb_60 < imb_300 + 0.08)
    )

    top_fade = (
        (ret_300 > 0.0010)
        & (ret_600 > 0.0014)
        & (pos_900 > 0.86)
        & fake_up
        & (vol_60_vs_900 > 1.08)
        & (imb_60 < imb_300 + 0.03)
    )
    bottom_fade = (
        (ret_300 < -0.0010)
        & (ret_600 < -0.0014)
        & (pos_900 < 0.14)
        & fake_down
        & (vol_60_vs_900 > 1.08)
        & (imb_60 > imb_300 - 0.03)
    )

    signal[trend_up | bottom_fade] = 1
    signal[trend_down | top_fade] = -1
    return signal


def mask_between(df: pd.DataFrame, start_ms: int, end_ms: int) -> pd.Series:
    return (df["second_ms"] >= start_ms) & (df["second_ms"] < end_ms)


def make_model(args: argparse.Namespace):
    if args.model == "hgb":
        return HistGradientBoostingClassifier(
            max_iter=args.hgb_iter,
            learning_rate=0.035,
            max_leaf_nodes=31,
            min_samples_leaf=220,
            l2_regularization=0.25,
            random_state=23,
        )
    if args.model == "extra":
        return ExtraTreesClassifier(
            n_estimators=args.tree_estimators,
            max_depth=args.extra_max_depth,
            min_samples_leaf=args.extra_min_samples_leaf,
            max_features=args.extra_max_features,
            class_weight="balanced",
            n_jobs=args.jobs,
            random_state=29,
        )
    if args.model == "xgb":
        if XGBClassifier is None:
            raise RuntimeError("xgboost is not installed in this Python environment")
        return XGBClassifier(
            n_estimators=args.tree_estimators,
            max_depth=5,
            learning_rate=0.035,
            min_child_weight=80,
            subsample=0.80,
            colsample_bytree=0.80,
            reg_lambda=2.5,
            reg_alpha=0.10,
            objective="binary:logistic",
            eval_metric="logloss",
            tree_method="hist",
            device=args.gpu_device,
            n_jobs=args.jobs,
            random_state=31,
        )
    if args.model == "cat":
        if CatBoostClassifier is None:
            raise RuntimeError("catboost is not installed in this Python environment")
        return CatBoostClassifier(
            iterations=args.tree_estimators,
            depth=6,
            learning_rate=0.035,
            l2_leaf_reg=8.0,
            loss_function="Logloss",
            eval_metric="Logloss",
            random_seed=37,
            task_type="GPU" if args.gpu_device == "cuda" else "CPU",
            devices="0" if args.gpu_device == "cuda" else None,
            thread_count=args.jobs,
            verbose=False,
        )
    raise ValueError(args.model)


def apply_execution_filter(df: pd.DataFrame, signal: np.ndarray, mode: str) -> np.ndarray:
    if mode == "none":
        return signal

    filtered = signal.copy()
    if mode == "block_up_falling_knife":
        up_falling_knife = (
            (filtered == 1)
            & (df["candle_body_share_60"].to_numpy() > 0.66)
            & (df["candle_lower_wick_share_60"].to_numpy() < 0.30)
        )
        filtered[up_falling_knife] = 0
        return filtered

    if mode == "block_up_solid_no_lower_wick":
        up_falling_knife = (
            (filtered == 1)
            & (df["candle_body_share_60"].to_numpy() > 0.70)
            & (df["candle_lower_wick_share_60"].to_numpy() < 0.25)
        )
        filtered[up_falling_knife] = 0
        return filtered

    if mode == "up_rejection_only":
        weak_up_reversal = (filtered == 1) & (df["candle_lower_wick_share_60"].to_numpy() < 0.30)
        filtered[weak_up_reversal] = 0
        return filtered

    raise ValueError(mode)


def backtest(
    df: pd.DataFrame,
    mask: pd.Series,
    move_prob: np.ndarray,
    up_prob: np.ndarray,
    move_th: float,
    dir_th: float,
    *,
    down_dir_th: float | None = None,
    delay: int,
    candidate_signal: np.ndarray | None = None,
    execution_filter: str = "none",
) -> dict:
    signal = np.zeros(len(df), dtype=np.int8)
    usable = mask.to_numpy() & np.isfinite(move_prob) & np.isfinite(up_prob) & (move_prob >= move_th)
    down_th = dir_th if down_dir_th is None else down_dir_th
    if candidate_signal is None:
        signal[usable & (up_prob >= dir_th)] = 1
        signal[usable & (up_prob <= 1.0 - down_th)] = -1
    else:
        allow_up = candidate_signal > 0
        allow_down = (candidate_signal < 0) | (candidate_signal == 2)
        signal[usable & allow_up & (up_prob >= dir_th)] = 1
        signal[usable & allow_down & (up_prob <= 1.0 - down_th)] = -1
    signal = apply_execution_filter(df, signal, execution_filter)

    close = df["close"].to_numpy()
    trades = wins = up = down = 0
    signed_ret = 0.0
    i = 0
    limit = len(df) - SETTLE_SECONDS - delay
    while i < limit:
        direction = int(signal[i])
        if direction == 0:
            i += 1
            continue
        entry = i + delay
        settle = entry + SETTLE_SECONDS
        future_ret = close[settle] / close[entry] - 1.0
        result = 1 if future_ret > 0 else -1
        won = direction == result
        trades += 1
        wins += int(won)
        up += int(direction > 0)
        down += int(direction < 0)
        signed_ret += future_ret * direction
        i += SETTLE_SECONDS

    losses = trades - wins
    return {
        "move_th": move_th,
        "dir_th": dir_th,
        "down_dir_th": down_th,
        "delay": delay,
        "trades": trades,
        "wins": wins,
        "losses": losses,
        "up": up,
        "down": down,
        "win_rate": wins / trades if trades else 0.0,
        "ev": (wins * WIN_USDT + losses * LOSS_USDT) / trades if trades else 0.0,
        "dir_ret_bp": signed_ret / trades * 10000 if trades else 0.0,
    }


def collect_trades(
    df: pd.DataFrame,
    mask: pd.Series,
    move_prob: np.ndarray,
    up_prob: np.ndarray,
    move_th: float,
    dir_th: float,
    *,
    down_dir_th: float | None = None,
    delay: int,
    fold: int,
    candidate_signal: np.ndarray | None = None,
    execution_filter: str = "none",
) -> list[dict]:
    signal = np.zeros(len(df), dtype=np.int8)
    usable = mask.to_numpy() & np.isfinite(move_prob) & np.isfinite(up_prob) & (move_prob >= move_th)
    down_th = dir_th if down_dir_th is None else down_dir_th
    if candidate_signal is None:
        signal[usable & (up_prob >= dir_th)] = 1
        signal[usable & (up_prob <= 1.0 - down_th)] = -1
    else:
        allow_up = candidate_signal > 0
        allow_down = (candidate_signal < 0) | (candidate_signal == 2)
        signal[usable & allow_up & (up_prob >= dir_th)] = 1
        signal[usable & allow_down & (up_prob <= 1.0 - down_th)] = -1
    signal = apply_execution_filter(df, signal, execution_filter)

    close = df["close"].to_numpy()
    records = []
    i = 0
    limit = len(df) - SETTLE_SECONDS - delay
    while i < limit:
        direction = int(signal[i])
        if direction == 0:
            i += 1
            continue
        entry = i + delay
        settle = entry + SETTLE_SECONDS
        future_ret = close[settle] / close[entry] - 1.0
        result = 1 if future_ret > 0 else -1
        row = df.iloc[i]
        records.append(
            {
                "fold": fold,
                "signal_ts": row["ts_utc"],
                "entry_ts": df.iloc[entry]["ts_utc"],
                "settle_ts": df.iloc[settle]["ts_utc"],
                "direction": "UP" if direction > 0 else "DOWN",
                "win": int(direction == result),
                "move_th": move_th,
                "up_dir_th": dir_th,
                "down_dir_th": down_th,
                "move_prob": float(move_prob[i]),
                "up_prob": float(up_prob[i]),
                "entry_price": float(close[entry]),
                "settle_price": float(close[settle]),
                "future_ret_bp": float(future_ret * 10000.0),
                "ret_60_bp": float(row["ret_60"] * 10000.0),
                "ret_180_bp": float(row["ret_180"] * 10000.0),
                "ret_300_bp": float(row["ret_300"] * 10000.0),
                "ret_600_bp": float(row["ret_600"] * 10000.0),
                "range_300_bp": float(row["range_ret_300"] * 10000.0),
                "range_600_bp": float(row["range_ret_600"] * 10000.0),
                "pos_300": float(row["pos_300"]),
                "pos_900": float(row["pos_900"]),
                "vol_60_vs_900": float(row["vol_60_vs_900"]),
                "body_share_60": float(row["candle_body_share_60"]),
                "upper_wick_60": float(row["candle_upper_wick_share_60"]),
                "lower_wick_60": float(row["candle_lower_wick_share_60"]),
            }
        )
        i += SETTLE_SECONDS
    return records


def thresholds(text: str) -> list[float]:
    return [float(item.strip()) for item in text.split(",") if item.strip()]


def select_row(rows: list[dict], min_trades: int, min_win_rate: float, min_ev: float) -> dict:
    eligible = [row for row in rows if row["trades"] >= min_trades]
    if min_win_rate > 0:
        eligible = [row for row in eligible if row["win_rate"] >= min_win_rate]
    if min_ev > -10:
        eligible = [row for row in eligible if row["ev"] >= min_ev]
    if not eligible:
        eligible = [row for row in rows if row["trades"] >= min_trades] or rows
    return max(eligible, key=lambda row: (row["ev"], row["win_rate"], row["trades"]))


def print_row(prefix: str, row: dict) -> None:
    dir_text = (
        f"up_dir={row['dir_th']:.2f} down_dir={row.get('down_dir_th', row['dir_th']):.2f}"
        if row.get("down_dir_th", row["dir_th"]) != row["dir_th"]
        else f"dir={row['dir_th']:.2f}"
    )
    print(
        f"{prefix} move={row['move_th']:.2f} {dir_text} "
        f"trades={row['trades']:>4} win={row['win_rate']*100:>5.1f}% "
        f"ev={row['ev']:>5.2f}U up={row['up']:>4} down={row['down']:>4} "
        f"dir_ret={row['dir_ret_bp']:>6.2f}bp",
        flush=True,
    )


def build_candidate_signal(df: pd.DataFrame, args: argparse.Namespace) -> np.ndarray | None:
    if args.structure_gate == "none":
        return None
    if args.structure_gate == "minute":
        return minute_structure_signal(df)
    raise ValueError(args.structure_gate)


def market_state_mask(df: pd.DataFrame, mode: str) -> pd.Series:
    if mode == "none":
        return pd.Series(True, index=df.index)

    ret_60 = df["ret_60"].abs()
    ret_180 = df["ret_180"].abs()
    ret_300 = df["ret_300"].abs()
    ret_600 = df["ret_600"].abs()
    pos_300 = df["pos_300"]
    pos_900 = df["pos_900"]
    range_300 = df["range_ret_300"] * 10000.0
    range_600 = df["range_ret_600"] * 10000.0
    vol_60_vs_900 = df["vol_60_vs_900"]
    body_share_60 = df["candle_body_share_60"]
    upper_wick_60 = df["candle_upper_wick_share_60"]
    lower_wick_60 = df["candle_lower_wick_share_60"]

    quiet = (range_300 < 4.0) | ((range_600 < 7.0) & (ret_300 < 0.00035))
    middle_chop = (
        (ret_300 < 0.00045)
        & (ret_600 < 0.00070)
        & (range_600 > 8.0)
        & pos_900.between(0.35, 0.65)
        & vol_60_vs_900.between(0.70, 1.35)
    )
    fast_extreme = (
        ((ret_60 > 0.00080) | (ret_180 > 0.00130) | (ret_300 > 0.00180))
        & ((pos_900 > 0.90) | (pos_900 < 0.10))
        & (vol_60_vs_900 > 1.35)
        & (body_share_60 > 0.45)
        & (upper_wick_60 < 0.45)
        & (lower_wick_60 < 0.45)
    )

    if mode == "basic":
        return ~(quiet | middle_chop)
    if mode == "no_extreme_chase":
        return ~(quiet | middle_chop | fast_extreme)
    if mode == "trend_mid":
        trend_mid = (
            (ret_180 > 0.00025)
            & (ret_300 > 0.00035)
            & pos_300.between(0.20, 0.80)
            & pos_900.between(0.12, 0.88)
            & (range_300 >= 4.0)
        )
        return trend_mid & ~fast_extreme
    raise ValueError(mode)


def rolling_two_stage(df: pd.DataFrame, features: list[str], feature_mask: pd.Series, args: argparse.Namespace) -> None:
    day_ms = 86_400_000
    start_ms = int(df["second_ms"].iloc[0])
    end_ms = int(df["second_ms"].iloc[-1])
    train_ms = args.train_days * day_ms
    valid_ms = args.valid_days * day_ms
    test_ms = args.test_days * day_ms
    step_ms = args.step_days * day_ms
    move_ths = thresholds(args.move_thresholds)
    dir_ths = thresholds(args.direction_thresholds)
    if args.up_direction_thresholds or args.down_direction_thresholds:
        up_dir_ths = thresholds(args.up_direction_thresholds) if args.up_direction_thresholds else dir_ths
        down_dir_ths = (
            thresholds(args.down_direction_thresholds) if args.down_direction_thresholds else dir_ths
        )
        direction_threshold_pairs = [(up_dt, down_dt) for up_dt in up_dir_ths for down_dt in down_dir_ths]
    else:
        direction_threshold_pairs = [(dt, dt) for dt in dir_ths]
    move_label = (df["future_ret"].abs() >= args.move_label_bp / 10000.0).astype("int8")
    candidate_signal = build_candidate_signal(df, args)
    state_mask = market_state_mask(df, args.market_state_filter)
    if args.market_state_filter != "none":
        print(
            f"market_state_filter={args.market_state_filter} usable_states={int(state_mask.sum())}",
            flush=True,
        )
    if candidate_signal is not None:
        up_candidates = int((candidate_signal > 0).sum())
        down_candidates = int(((candidate_signal < 0) | (candidate_signal == 2)).sum())
        print(f"structure_gate={args.structure_gate} candidates up={up_candidates} down={down_candidates}", flush=True)

    print(
        f"two-stage rolling model={args.model} train={args.train_days}d valid={args.valid_days}d "
        f"test={args.test_days}d step={args.step_days}d move_label={args.move_label_bp:.2f}bp "
        f"breakeven={BREAKEVEN*100:.2f}%",
        flush=True,
    )

    fold = 0
    rows = []
    delay_rows = []
    trade_records = []
    cursor = start_ms
    while cursor + train_ms + valid_ms + test_ms <= end_ms:
        train = mask_between(df, cursor, cursor + train_ms) & feature_mask
        valid = (
            mask_between(df, cursor + train_ms, cursor + train_ms + valid_ms)
            & feature_mask
            & state_mask
        )
        test = mask_between(
            df,
            cursor + train_ms + valid_ms,
            cursor + train_ms + valid_ms + test_ms,
        ) & feature_mask & state_mask
        train_idx = np.flatnonzero(train.to_numpy())
        train_idx = train_idx[train_idx % args.sample_stride == 0]
        dir_train_idx = train_idx[move_label.iloc[train_idx].to_numpy() == 1]
        if len(train_idx) < 20_000 or len(dir_train_idx) < 5_000 or valid.sum() == 0 or test.sum() == 0:
            cursor += step_ms
            continue

        x_train = df.iloc[train_idx][features].astype("float32")
        x_dir_train = df.iloc[dir_train_idx][features].astype("float32")
        x_valid = df.loc[valid, features].astype("float32")
        x_test = df.loc[test, features].astype("float32")

        move_model = deepcopy(make_model(args))
        dir_model = deepcopy(make_model(args))
        move_model.fit(x_train, move_label.iloc[train_idx].astype("int8"))
        dir_model.fit(x_dir_train, df.iloc[dir_train_idx]["label_up"].astype("int8"))

        valid_idx = np.flatnonzero(valid.to_numpy())
        test_idx = np.flatnonzero(test.to_numpy())
        valid_move_prob = np.full(len(df), np.nan, dtype=np.float32)
        valid_up_prob = np.full(len(df), np.nan, dtype=np.float32)
        test_move_prob = np.full(len(df), np.nan, dtype=np.float32)
        test_up_prob = np.full(len(df), np.nan, dtype=np.float32)
        valid_move_prob[valid_idx] = move_model.predict_proba(x_valid)[:, 1]
        valid_up_prob[valid_idx] = dir_model.predict_proba(x_valid)[:, 1]
        test_move_prob[test_idx] = move_model.predict_proba(x_test)[:, 1]
        test_up_prob[test_idx] = dir_model.predict_proba(x_test)[:, 1]

        valid_rows = [
            backtest(
                df,
                valid,
                valid_move_prob,
                valid_up_prob,
                mt,
                up_dt,
                down_dir_th=down_dt,
                delay=args.select_delay_seconds,
                candidate_signal=candidate_signal,
                execution_filter=(
                    "none" if args.execution_filter_mode == "test_only" else args.execution_filter
                ),
            )
            for mt in move_ths
            for up_dt, down_dt in direction_threshold_pairs
        ]
        best = select_row(
            valid_rows,
            args.min_select_trades,
            args.min_select_win_rate,
            args.min_select_ev,
        )
        test_row = backtest(
            df,
            test,
            test_move_prob,
            test_up_prob,
            best["move_th"],
            best["dir_th"],
            down_dir_th=best.get("down_dir_th"),
            delay=0,
            candidate_signal=candidate_signal,
            execution_filter=args.execution_filter,
        )
        delay_row = backtest(
            df,
            test,
            test_move_prob,
            test_up_prob,
            best["move_th"],
            best["dir_th"],
            down_dir_th=best.get("down_dir_th"),
            delay=args.delay_seconds,
            candidate_signal=candidate_signal,
            execution_filter=args.execution_filter,
        )
        fold += 1
        if args.dump_trades:
            trade_records.extend(
                collect_trades(
                    df,
                    test,
                    test_move_prob,
                    test_up_prob,
                    best["move_th"],
                    best["dir_th"],
                    down_dir_th=best.get("down_dir_th"),
                    delay=args.delay_seconds,
                    fold=fold,
                    candidate_signal=candidate_signal,
                    execution_filter=args.execution_filter,
                )
            )
        rows.append(test_row)
        delay_rows.append(delay_row)
        print(
            f"fold={fold} window={df.loc[test, 'ts_utc'].iloc[0]} -> {df.loc[test, 'ts_utc'].iloc[-1]}",
            flush=True,
        )
        print_row("  valid", best)
        print_row("  test ", test_row)
        print_row(f"  delay{args.delay_seconds}", delay_row)
        cursor += step_ms

    trades = sum(row["trades"] for row in rows)
    wins = sum(row["wins"] for row in rows)
    losses = trades - wins
    ev = (wins * WIN_USDT + losses * LOSS_USDT) / trades if trades else 0.0
    up = sum(row["up"] for row in rows)
    down = sum(row["down"] for row in rows)
    print(
        f"summary folds={fold} trades={trades} wins={wins} losses={losses} "
        f"win={wins/trades*100 if trades else 0:.1f}% ev={ev:.2f}U up={up} down={down}",
        flush=True,
    )
    delay_trades = sum(row["trades"] for row in delay_rows)
    delay_wins = sum(row["wins"] for row in delay_rows)
    delay_losses = delay_trades - delay_wins
    delay_ev = (
        (delay_wins * WIN_USDT + delay_losses * LOSS_USDT) / delay_trades
        if delay_trades
        else 0.0
    )
    delay_up = sum(row["up"] for row in delay_rows)
    delay_down = sum(row["down"] for row in delay_rows)
    print(
        f"delay{args.delay_seconds} summary folds={fold} trades={delay_trades} "
        f"wins={delay_wins} losses={delay_losses} "
        f"win={delay_wins/delay_trades*100 if delay_trades else 0:.1f}% "
        f"ev={delay_ev:.2f}U up={delay_up} down={delay_down}",
        flush=True,
    )
    if args.dump_trades:
        pd.DataFrame(trade_records).to_csv(args.dump_trades, index=False)
        print(f"dumped_trades={len(trade_records)} path={args.dump_trades}", flush=True)


def parse_args() -> argparse.Namespace:
    default_psql = shutil.which("psql") or r"D:\pgsql\bin\psql.exe"
    parser = argparse.ArgumentParser()
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--db", default="event_signal_train")
    parser.add_argument("--db-user", default="postgres")
    parser.add_argument("--psql", default=default_psql)
    parser.add_argument("--model", choices=["hgb", "extra", "xgb", "cat"], default="hgb")
    parser.add_argument("--gpu-device", choices=["cpu", "cuda"], default="cpu")
    parser.add_argument("--sample-stride", type=int, default=8)
    parser.add_argument("--hgb-iter", type=int, default=220)
    parser.add_argument("--tree-estimators", type=int, default=260)
    parser.add_argument("--extra-max-depth", type=int, default=18)
    parser.add_argument("--extra-min-samples-leaf", type=int, default=100)
    parser.add_argument("--extra-max-features", default="sqrt")
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--move-label-bp", type=float, default=4.0)
    parser.add_argument("--settle-seconds", type=int, default=SETTLE_SECONDS)
    parser.add_argument("--move-thresholds", default="0.52,0.56,0.60,0.64,0.68,0.72")
    parser.add_argument("--direction-thresholds", default="0.54,0.56,0.58,0.60,0.62,0.64,0.66")
    parser.add_argument("--up-direction-thresholds", default="")
    parser.add_argument("--down-direction-thresholds", default="")
    parser.add_argument(
        "--structure-gate",
        choices=["none", "minute"],
        default="none",
    )
    parser.add_argument(
        "--execution-filter",
        choices=[
            "none",
            "block_up_falling_knife",
            "block_up_solid_no_lower_wick",
            "up_rejection_only",
        ],
        default="none",
    )
    parser.add_argument(
        "--execution-filter-mode",
        choices=["select_and_test", "test_only"],
        default="select_and_test",
    )
    parser.add_argument(
        "--market-state-filter",
        choices=["none", "basic", "no_extreme_chase", "trend_mid"],
        default="none",
    )
    parser.add_argument("--min-select-trades", type=int, default=80)
    parser.add_argument("--min-select-win-rate", type=float, default=0.0)
    parser.add_argument("--min-select-ev", type=float, default=-10.0)
    parser.add_argument("--delay-seconds", type=int, default=5)
    parser.add_argument("--select-delay-seconds", type=int, default=0)
    parser.add_argument("--label-delay-seconds", type=int, default=0)
    parser.add_argument(
        "--load-days",
        type=int,
        default=0,
        help="Only load the latest N days from the local 1s price table; 0 loads all rows.",
    )
    parser.add_argument("--train-days", type=int, default=60)
    parser.add_argument("--valid-days", type=int, default=10)
    parser.add_argument("--test-days", type=int, default=10)
    parser.add_argument("--step-days", type=int, default=10)
    parser.add_argument("--dump-trades", default="")
    return parser.parse_args()


def main() -> int:
    warnings.simplefilter("ignore", PerformanceWarning)
    args = parse_args()
    set_settle_seconds(args.settle_seconds)
    df = load_prices(args)
    df, features = add_features(df)
    apply_label_delay(df, args.label_delay_seconds)
    feature_mask = df[features].notna().all(axis=1) & df["future_ret"].notna()
    print(
        f"rows={len(df)} features={len(features)} usable={int(feature_mask.sum())} "
        f"{df['ts_utc'].iloc[0]} -> {df['ts_utc'].iloc[-1]}",
        flush=True,
    )
    rolling_two_stage(df, features, feature_mask, args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
