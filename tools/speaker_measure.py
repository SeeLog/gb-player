#!/usr/bin/env python3
"""Measure a console's speaker against gb-player's line out (see docs/GBC_SPEAKER.md).

Usage:
  speaker_measure.py response --render line.wav --recording rec.wav --noise silence.wav --out rec.csv
  speaker_measure.py fit rec.csv [more.csv ...] --render line.wav --out fit.json
  speaker_measure.py noise --on on.wav --off off.wav --recording rec.wav --render line.wav --fit fit.json
  speaker_measure.py plot rec.csv [more.csv ...] [--fit fit.json] --title ... --out plot.png

response  compares a recording of a song from the speaker with the same song rendered by
          `gb-player --wav <rom> <song#> line.wav 1`, and writes the level per frequency,
          relative to 1-2 kHz, with the bands that did not rise out of the noise marked.
          Rendered with `... 1 gbc-speaker` instead, it shows what the profile still misses.
fit       fits the filters of effects.rs to one or more of those, prints them as Rust, and
          prints the gains that would match the line out's loudness or peaks.
noise     finds the tones that a silent console adds to the room's own noise, and prints them
          as Rust at their level relative to the filtered music.
plot      draws responses (and the fitted filters) to a PNG.

Recordings are trimmed by a second at both ends (--trim) to drop the clicks that start and stop
them. Needs numpy and scipy (and matplotlib to plot).
"""
import argparse
import json
import math
import sys

import numpy as np
from scipy.io import wavfile
from scipy.ndimage import maximum_filter, median_filter
from scipy.optimize import least_squares
from scipy.signal import butter, lfilter, resample_poly, sosfilt, sosfiltfilt, stft, welch

RATE = 48000
# gb-player --wav renders at the player's default volume.
RENDER_VOLUME = 0.8
FRAME_HZ = 4194304 / 70224
NPER = 4096


def load(path, trim=0.0):
    """A WAV file as mono floats at RATE, without `trim` seconds at both ends."""
    rate, x = wavfile.read(path)
    if x.dtype.kind == "i":
        x = x / float(np.iinfo(x.dtype).max + 1)
    x = x.astype(np.float64)
    if x.ndim == 2:
        x = x.mean(axis=1)  # a single speaker plays L+R
    if rate != RATE:
        g = math.gcd(RATE, rate)
        x = resample_poly(x, RATE // g, rate // g)
    n = int(trim * RATE)
    return x[n:len(x) - n] if n else x


# --- Lining a recording up with the render ------------------------------------------------------

BAND = butter(4, [400, 4000], "bandpass", fs=RATE, output="sos")


def lag_of(chunk, ref_b, start=None, search=None):
    """Where `chunk` is in the band-passed render, by FFT cross-correlation, and how well."""
    c = sosfiltfilt(BAND, chunk)
    base = 0 if start is None else max(0, start - search)
    r = ref_b if start is None else ref_b[base:start + len(c) + search]
    if len(r) < len(c):
        return 0, 0.0
    nfft = 1 << (len(r) + len(c) - 1).bit_length()
    xc = np.fft.irfft(np.fft.rfft(r, nfft) * np.conj(np.fft.rfft(c, nfft)), nfft)[:len(r) - len(c) + 1]
    i = int(np.argmax(xc))
    peak = xc[i] / (np.linalg.norm(c) * np.linalg.norm(r[i:i + len(c)]) + 1e-12)
    return base + i, peak


def align(rec, ref):
    """The render lined up with the recording. Notes land up to a frame apart on the console and
    in the render, so the waveforms only partly match; the offset is the median over 3 s chunks,
    leaving out chunks that locked onto a repeat of the same phrase."""
    ref_b = sosfiltfilt(BAND, ref)
    l0, _ = lag_of(rec[:RATE * 8], ref_b)
    w = RATE * 3
    pts = []
    for s in range(0, len(rec) - w, w // 2):
        lag, peak = lag_of(rec[s:s + w], ref_b, int(l0 + s), RATE // 10)
        pts.append((lag - s, peak))
    pts = np.array(pts)
    good = (pts[:, 1] > 0.15) & (np.abs(pts[:, 0] - np.median(pts[:, 0])) < RATE * 0.05)
    if not good.any():
        sys.exit("could not find the recording in the render; is it the same song?")
    # A whole number of samples: shifting by a fraction would interpolate, which dulls the
    # render's highs (by 3 dB at 12.5 kHz for half a sample) and so brightens the response.
    offset = int(np.median(pts[good, 0]))
    print(f"recording starts {offset / RATE:.3f} s into the render "
          f"({good.sum()}/{len(pts)} chunks agree, median correlation {np.median(pts[:, 1]):.2f})")
    shifted = np.zeros(len(rec))
    part = ref[max(offset, 0):offset + len(rec)]
    shifted[max(-offset, 0):max(-offset, 0) + len(part)] = part
    return shifted, offset


# --- response -----------------------------------------------------------------------------------

def smooth(f, h, frac=6):
    """Mean over a 1/frac-octave band around each frequency."""
    out = np.full_like(h, np.nan)
    for i, fc in enumerate(f):
        if fc >= 20:
            m = (f >= fc * 2 ** (-0.5 / frac)) & (f <= fc * 2 ** (0.5 / frac))
            out[i] = np.mean(h[m])
    return out


def response(render, recording, noise, trim):
    rec = load(recording, trim)
    ref, _ = align(rec, load(render))
    f, _, R = stft(rec, RATE, nperseg=NPER, noverlap=NPER * 3 // 4)
    _, _, E = stft(ref, RATE, nperseg=NPER, noverlap=NPER * 3 // 4)
    pr, pe = np.abs(R) ** 2, np.abs(E) ** 2
    # The noise's power per STFT frame, from its density.
    _, pn = welch(load(noise, trim), RATE, nperseg=NPER)
    win = np.hanning(NPER)
    nf = pn[:, None] * RATE * np.sum(win ** 2) / np.sum(win) ** 2 / 2
    # Frames much louder than the render predicts carry sounds it does not have (sound effects).
    mid = (f > 400) & (f < 4000)
    ratio = np.log10(pr[mid].sum(0) + 1e-20) - np.log10(pe[mid].sum(0) + 1e-20)
    keep = np.abs(ratio - np.median(ratio)) < 0.3  # about 3 dB
    print(f"frames kept {keep.sum()}/{len(keep)}")
    num = np.sum(np.maximum(pr[:, keep] - nf, 0), 1)
    den = np.sum(pe[:, keep], 1)
    snr = np.sum(pr[:, keep], 1) / (nf[:, 0] * keep.sum() + 1e-30)
    h, snr = smooth(f, num / (den + 1e-30)), smooth(f, snr)
    level = 10 * np.log10(h / np.nanmean(h[(f > 1000) & (f < 2000)]))
    return f, level, 10 * np.log10(snr) > 6


def cmd_response(a):
    f, level, valid = response(a.render, a.recording, a.noise, a.trim)
    np.savetxt(a.out, np.column_stack([f, level, valid]), delimiter=",", fmt=["%.3f", "%.3f", "%d"],
               header="hz,level_db,measured", comments="")
    print("\n     Hz   dB (relative to 1-2 kHz; -- where lost in the noise)")
    for fc in [100, 200, 315, 400, 500, 630, 800, 1000, 1600, 2000, 2500, 3150, 4000, 5000,
               6300, 8000, 10000, 12500, 16000]:
        i = int(np.argmin(np.abs(f - fc)))
        print(f"  {fc:6d}  {level[i]:6.1f}" if valid[i] else f"  {fc:6d}      --")


def read_csv(path):
    d = np.loadtxt(path, delimiter=",", skiprows=1)
    return d[:, 0], d[:, 1], d[:, 2] > 0


# --- fit ----------------------------------------------------------------------------------------

def biquad(kind, f0, q, gain_db=0.0, fs=RATE):
    """The RBJ cookbook biquad as (b, a) with a[0] = 1, as effects.rs builds it."""
    w = 2 * np.pi * min(f0, fs * 0.45) / fs
    cw, sw = np.cos(w), np.sin(w)
    al = sw / (2 * q)
    amp = 10 ** (gain_db / 40)
    if kind == "HighPass":
        b, a = [(1 + cw) / 2, -(1 + cw), (1 + cw) / 2], [1 + al, -2 * cw, 1 - al]
    elif kind == "LowPass":
        b, a = [(1 - cw) / 2, 1 - cw, (1 - cw) / 2], [1 + al, -2 * cw, 1 - al]
    else:
        b, a = [1 + al * amp, -2 * cw, 1 - al * amp], [1 + al / amp, -2 * cw, 1 - al / amp]
    return np.array(b) / a[0], np.array(a) / a[0]


def mag_db(filters, freqs):
    z = np.exp(-2j * np.pi * np.asarray(freqs) / RATE)
    h = np.ones_like(z)
    for kind, f0, q, g in filters:
        b, a = biquad(kind, f0, q, g)
        h = h * (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
    return 20 * np.log10(np.abs(h))


def sos_of(filters):
    return np.array([np.r_[biquad(*f)] for f in filters]).reshape(len(filters), 6)


# Starting points for the GBC speaker; another speaker may want others.
HIGH_PASSES = [(650, 1.2), (450, 0.7)]
PEAKS = [(1000, 3, 1.0), (1800, -4, 2.0), (2350, 4, 3.0), (3700, 13, 2.0), (6500, 5, 3.0), (8200, 7, 3.0)]
LOW_PASS = (11000, 0.9)


def unpack(p):
    filters = [("HighPass", p[0], p[1], 0.0), ("HighPass", p[2], p[3], 0.0)]
    for i in range(len(PEAKS)):
        fc, g, q = p[4 + 3 * i:7 + 3 * i]
        filters.append(("Peak", fc, q, g))
    filters.append(("LowPass", p[-3], p[-2], 0.0))
    return filters, p[-1]


def fit(paths):
    """Filters whose response, plus a level, best matches the mean of the measurements."""
    f, target, n = None, 0, 0
    for path in paths:
        f, level, valid = read_csv(path)
        target = target + np.where(valid, level, 0)
        n = n + valid
    target = np.where(n > 0, target / np.maximum(n, 1), np.nan)
    idx = np.unique([int(np.argmin(np.abs(f - g))) for g in np.geomspace(280, 14000, 160)])
    idx = idx[~np.isnan(target[idx])]
    fx, ty = f[idx], target[idx]
    p0, lo, hi = [], [], []
    for fc, q in HIGH_PASSES:
        p0 += [fc, q]
        lo += [fc * 0.4, 0.4]
        hi += [1200, 4.0]
    for fc, g, q in PEAKS:
        p0 += [fc, g, q]
        lo += [fc * 0.85, -15, 0.4]
        hi += [fc * 1.15, 15, 4.0]
    p0 += [LOW_PASS[0], LOW_PASS[1], 0]
    lo += [6000, 0.4, -20]
    hi += [20000, 3.0, 20]

    def error(p):
        filters, level = unpack(p)
        return mag_db(filters, fx) + level - ty

    def resid(p):
        # A little pull towards small bells: a broad boost and a lower level can otherwise
        # trade off against each other without end.
        gains = [p[5 + 3 * i] for i in range(len(PEAKS))]
        return np.r_[error(p), 0.1 * np.array(gains)]

    p = least_squares(resid, p0, bounds=(lo, hi)).x
    err = error(p)
    print(f"fit error: rms {np.sqrt(np.mean(err ** 2)):.2f} dB, max {np.max(np.abs(err)):.2f} dB")
    return unpack(p)


def k_weighted(x):
    """ITU-R BS.1770 K-weighting at 48 kHz, for loudness."""
    x = lfilter([1.53512485958697, -2.69169618940638, 1.19839281085285],
                [1.0, -1.69065929318241, 0.73248077421585], x)
    return lfilter([1.0, -2.0, 1.0], [1.0, -1.99004745483398, 0.99007225036621], x)


def db(x):
    return 20 * np.log10(x)


def cmd_fit(a):
    filters, _ = fit(a.measurements)
    print("\nconst GBC_SPEAKER_FILTERS: &[(FilterKind, f64, f64, f64)] = &[")
    for kind, fc, q, g in filters:
        print(f"    (FilterKind::{kind}, {fc:.1f}, {q:.3f}, {g:.2f}),")
    print("];")
    line = load(a.render) / RENDER_VOLUME
    spk = sosfilt(sos_of(filters), line)
    rms = lambda x: np.sqrt(np.mean(x ** 2))  # noqa: E731
    loud = lambda x: rms(k_weighted(x))  # noqa: E731
    peak = lambda x: np.max(np.abs(x))  # noqa: E731
    print("\nThe gain after the filters (GBC_SPEAKER_GAIN_DB) that matches the line out's")
    print(f"  rms:      {db(rms(line) / rms(spk)):+.2f} dB")
    print(f"  loudness: {db(loud(line) / loud(spk)):+.2f} dB")
    print(f"  peaks:    {db(peak(line) / peak(spk)):+.2f} dB")
    json.dump({"filters": filters}, open(a.out, "w"), indent=1)


# --- noise --------------------------------------------------------------------------------------

def cmd_noise(a):
    filters = [tuple(f) for f in json.load(open(a.fit))["filters"]]
    # The level of the music on its way through the filters, against the recording's.
    rec = load(a.recording, a.trim)
    music = sosfilt(sos_of(filters), load(a.render) / RENDER_VOLUME)
    _, offset = align(rec, load(a.render))
    music = music[offset:offset + len(rec)]
    n = min(len(rec), len(music))
    scale = np.sqrt(np.mean(music[:n] ** 2)) / np.sqrt(np.mean(rec[:n] ** 2))

    fq, pon = welch(load(a.on, a.trim), RATE, nperseg=RATE * 4)  # 0.25 Hz bins
    _, poff = welch(load(a.off, a.trim), RATE, nperseg=RATE * 4)
    df = fq[1] - fq[0]

    def power(p, fc):
        return np.sum(p[np.abs(fq - fc) <= 1.0]) * df

    def floor(p, fc):
        m = (np.abs(fq - fc) > 3) & (np.abs(fq - fc) < 25)
        return np.median(p[m]) * np.sum(np.abs(fq - fc) <= 1.0) * df

    def amplitude(fc):
        return np.sqrt(2 * max(power(pon, fc) - floor(pon, fc), 0)) * scale

    print(f"total noise: console on {10 * np.log10(np.sum(pon) * df):.1f} dBFS, "
          f"off {10 * np.log10(np.sum(poff) * df):.1f} dBFS")
    print("\nconst GBC_INTERFERENCE: &[Tone] = &[")
    for h in range(1, int(20000 / FRAME_HZ)):
        fc = h * FRAME_HZ
        on = power(pon, fc)
        if on > 6 * floor(pon, fc) and on > 4 * power(poff, fc):
            print(f"    harmonic({h}, {amplitude(fc):.2e}),")
    # Other tones: peaks far above their surroundings with the console on only, away from the
    # frame-rate harmonics.
    surroundings = median_filter(pon, size=201)
    peaks = (pon > 100 * surroundings) & (pon == maximum_filter(pon, size=17))
    for i in np.flatnonzero(peaks):
        fc = fq[i]
        near_harmonic = abs(fc / FRAME_HZ - round(fc / FRAME_HZ)) * FRAME_HZ < 3
        if 100 < fc < 20000 and not near_harmonic and power(pon, fc) > 10 * power(poff, fc):
            print(f"    Tone {{ harmonic: 0, hz: {fc:.1f}, amplitude: {amplitude(fc):.2e} }},")
    print("];")


# --- plot ---------------------------------------------------------------------------------------

def cmd_plot(a):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.ticker import FixedFormatter, FixedLocator, NullLocator

    # A font with Japanese in it, among those installed.
    installed = {f.name for f in matplotlib.font_manager.fontManager.ttflist}
    japanese = [f for f in ["Yu Gothic", "Meiryo", "Noto Sans CJK JP", "Hiragino Sans"] if f in installed]
    plt.rcParams["font.family"] = japanese + ["sans-serif"]
    colors = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100"]
    fig, ax = plt.subplots(figsize=(9, 4.6), dpi=150)
    fig.patch.set_facecolor("#fcfcfb")
    ax.set_facecolor("#fcfcfb")
    labels = a.labels or a.measurements
    for path, label, color in zip(a.measurements, labels, colors):
        f, level, valid = read_csv(path)
        m = (f >= 100) & (f <= a.max_hz)
        ax.plot(f[m], np.where(valid, level, np.nan)[m], color=color, lw=2, label=label)
    if a.fit:
        filters = [tuple(x) for x in json.load(open(a.fit))["filters"]]
        ff = np.geomspace(100, 16000, 600)
        mf = mag_db(filters, ff)
        ref = np.mean(mag_db(filters, np.geomspace(1000, 2000, 50)))
        ax.plot(ff, mf - ref, color="#0b0b0b", lw=1.5, ls="--", label=a.fit_label)
    ax.axhline(0, color="#c9c8c2", lw=1, zorder=0)
    ax.set_xscale("log")
    ticks = [100, 200, 500, 1000, 2000, 5000, 10000, 16000]
    ax.xaxis.set_major_locator(FixedLocator(ticks))
    ax.xaxis.set_major_formatter(FixedFormatter([f"{t // 1000}k" if t >= 1000 else str(t) for t in ticks]))
    ax.xaxis.set_minor_locator(NullLocator())
    ax.set_xlim(100, 20000)
    if a.ylim:
        ax.set_ylim(*a.ylim)
    ax.grid(True, axis="y", color="#e6e5e0", lw=0.8)
    for s in ["top", "right"]:
        ax.spines[s].set_visible(False)
    for s in ["left", "bottom"]:
        ax.spines[s].set_color("#c9c8c2")
    ax.tick_params(colors="#52514e", labelsize=9)
    ax.set_xlabel(a.xlabel, color="#52514e", fontsize=10)
    ax.set_ylabel(a.ylabel, color="#52514e", fontsize=10)
    ax.set_title(a.title, color="#0b0b0b", fontsize=11, loc="left")
    ax.legend(frameon=False, fontsize=9, loc="lower right", labelcolor="#52514e")
    if a.note:
        ax.text(0.0, -0.21, a.note, transform=ax.transAxes, color="#52514e", fontsize=8)
    fig.tight_layout()
    fig.savefig(a.out, facecolor=fig.get_facecolor())


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    r = sub.add_parser("response", help="a recording's level per frequency, against the render")
    r.add_argument("--render", required=True, help="the song from gb-player --wav")
    r.add_argument("--recording", required=True, help="the same song from the speaker")
    r.add_argument("--noise", required=True, help="silence recorded the same way")
    r.add_argument("--out", required=True, help="CSV to write")
    r.add_argument("--trim", type=float, default=1.0, help="seconds to drop at both ends")
    r.set_defaults(run=cmd_response)

    f = sub.add_parser("fit", help="fit the speaker filters to responses")
    f.add_argument("measurements", nargs="+", help="CSVs from response")
    f.add_argument("--render", required=True, help="a line render, to compare the gains with")
    f.add_argument("--out", required=True, help="JSON to write the filters to")
    f.set_defaults(run=cmd_fit)

    n = sub.add_parser("noise", help="the console's own tones")
    n.add_argument("--on", required=True, help="silence with the console on")
    n.add_argument("--off", required=True, help="silence with the console off")
    n.add_argument("--recording", required=True, help="a song recorded like --on, for the level")
    n.add_argument("--render", required=True, help="the same song from gb-player --wav")
    n.add_argument("--fit", required=True, help="the filters from fit")
    n.add_argument("--trim", type=float, default=1.0, help="seconds to drop at both ends")
    n.set_defaults(run=cmd_noise)

    p = sub.add_parser("plot", help="draw responses")
    p.add_argument("measurements", nargs="+", help="CSVs from response")
    p.add_argument("--labels", nargs="+")
    p.add_argument("--fit", help="also draw the filters from fit")
    p.add_argument("--fit-label", default="filters")
    p.add_argument("--title", default="")
    p.add_argument("--xlabel", default="Hz")
    p.add_argument("--ylabel", default="dB")
    p.add_argument("--note", default="")
    p.add_argument("--ylim", type=float, nargs=2)
    p.add_argument("--max-hz", type=float, default=14000,
                   help="draw up to here; above, only stray bins rise out of the noise")
    p.add_argument("--out", required=True, help="PNG to write")
    p.set_defaults(run=cmd_plot)

    a = ap.parse_args()
    a.run(a)


if __name__ == "__main__":
    main()
